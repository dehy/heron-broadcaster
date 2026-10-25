// SPDX-License-Identifier: AGPL-3.0-or-later

//! `heron-engine`: real-time playout (decks, mixer, encoders, outputs).
//!
//! It never connects to the database and must keep running when every other
//! service is down. `heron-engine ctl` is a command-line client for its gRPC
//! control interface.

mod ctl;

use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::Context;
use clap::{Parser, Subcommand};
use heron_engine::engine::{Engine, EngineHealth, EngineOptions};
use heron_engine::grpc::EngineService;
use heron_engine::handover::{self, Followed, Probe};
use heron_service::{LogArgs, MetricsArgs};

const SERVICE: &str = "heron-engine";
/// How long a shutdown waits for open connections to close.
const SHUTDOWN_GRACE: Duration = Duration::from_secs(2);

#[derive(Debug, Parser)]
#[command(name = SERVICE, version, about = "Heron Broadcaster playout engine")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,

    /// Address of the ops HTTP server (`/healthz`, `/metrics`).
    #[arg(
        long,
        env = "HERON_HTTP_ADDR",
        default_value = "0.0.0.0:8081",
        global = true
    )]
    http_addr: SocketAddr,

    /// Address of the gRPC control interface, used by heron-api.
    #[arg(long, env = "HERON_GRPC_ADDR", default_value = "0.0.0.0:50051")]
    grpc_addr: SocketAddr,

    /// Root of the music library; queued track paths are relative to it.
    #[arg(
        long,
        env = "HERON_LIBRARY_DIR",
        default_value = "/music",
        global = true
    )]
    library_dir: PathBuf,

    /// Directory of tracks played when the queue runs dry or the API is unreachable.
    #[arg(
        long,
        env = "HERON_FALLBACK_DIR",
        default_value = "/var/lib/heron/fallback"
    )]
    fallback_dir: PathBuf,

    /// Queue entries whose file is read into memory ahead of time, so a slow
    /// or unreachable library does not interrupt the air.
    #[arg(long, env = "HERON_PRELOAD_TRACKS", default_value_t = heron_engine::player::PRELOAD_TRACKS)]
    preload_tracks: usize,

    /// Play the master bus on the local sound card (builds with the `monitor` feature).
    #[arg(long, env = "HERON_MONITOR")]
    monitor: bool,

    /// Directory where the engine persists its state (configuration, queue) between restarts.
    #[arg(long, env = "HERON_STATE_DIR", default_value = "/var/lib/heron/engine")]
    state_dir: PathBuf,

    #[command(flatten)]
    log: LogArgs,

    #[command(flatten)]
    metrics: MetricsArgs,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Exit 0 if the running engine answers `/healthz` (used by Docker).
    Healthcheck,
    /// Control a running engine.
    Ctl(ctl::Args),
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Some(Command::Healthcheck) => {
            heron_service::probe::probe(cli.http_addr, "/healthz", Duration::from_secs(3))?;
            Ok(())
        }
        Some(Command::Ctl(args)) => ctl::run(args, &cli.library_dir),
        None => run(cli),
    }
}

#[tokio::main]
async fn run(cli: Cli) -> anyhow::Result<()> {
    let _telemetry = heron_service::telemetry::init_tracing(&cli.log, SERVICE)?;
    let metrics = heron_service::ops::install_metrics(SERVICE, &cli.metrics)?;

    std::fs::create_dir_all(&cli.state_dir).with_context(|| {
        format!(
            "cannot create the state directory {}",
            cli.state_dir.display()
        )
    })?;
    // An engine running already hands over to this one (an upgrade).
    let state_dir = cli.state_dir.clone();
    let probe = tokio::task::spawn_blocking(move || handover::probe(&state_dir)).await?;
    let (follow, standby) = match probe {
        Probe::Alone => (None, false),
        Probe::Standby(channel) => {
            tracing::info!("an engine is on air: standing by to take over from it");
            (Some(channel), true)
        }
        Probe::Incompatible { protocol, version } => {
            tracing::warn!(
                protocol,
                version,
                "the engine on air cannot hand over to this one: starting once it stops"
            );
            (None, true)
        }
    };
    let engine = Engine::start(
        EngineOptions {
            fallback_dir: Some(cli.fallback_dir.clone()),
            preload_tracks: cli.preload_tracks,
            exit_when_stuck: true,
            monitor: cli.monitor,
            standby,
            ..EngineOptions::new(cli.library_dir.clone(), cli.state_dir.clone())
        },
        tokio::runtime::Handle::current(),
    )?;
    tracing::info!(
        library_dir = %cli.library_dir.display(),
        state_dir = %cli.state_dir.display(),
        "engine ready"
    );

    let health = EngineHealth(std::sync::Arc::clone(&engine));
    let router = heron_service::ops_router(SERVICE, metrics, health);
    let http = tokio::spawn(heron_service::serve(cli.http_addr, router));

    // A standby goes on air once the active engine hands over, or stops.
    let takeover = async {
        if standby {
            let following = std::sync::Arc::clone(&engine);
            let state_dir = cli.state_dir.clone();
            let followed = tokio::task::spawn_blocking(move || {
                let followed = match follow {
                    Some(channel) => handover::follow(&following, channel),
                    None => Followed::GivenUp("the engine on air speaks another protocol".to_owned()),
                };
                if let Followed::GivenUp(reason) = &followed {
                    tracing::warn!(%reason, "nothing handed over: starting once the other engine stops");
                    handover::wait_until_alone(&state_dir);
                    following.cold_start();
                }
                followed
            })
            .await?;
            if let Followed::TookOver = followed {
                tracing::info!("took over from the previous engine");
            }
        }
        anyhow::Ok(
            handover::Listener::bind(&cli.state_dir)
                .context("cannot listen for a standby engine")?,
        )
    };
    let listener = tokio::select! {
        listener = takeover => listener?,
        () = heron_service::shutdown_signal() => {
            // A standby stops without touching the state: it is not its own.
            tracing::info!("stopping before taking over");
            return Ok(());
        }
    };

    let grpc = tonic::transport::Server::builder()
        .add_service(EngineService::server(std::sync::Arc::clone(&engine)))
        .serve_with_shutdown(cli.grpc_addr, heron_service::shutdown_signal());
    tracing::info!(addr = %cli.grpc_addr, "grpc server listening");

    let servers = async {
        let (grpc, http) = tokio::join!(grpc, http);
        grpc.context("grpc server failed")?;
        http??;
        anyhow::Ok(())
    };
    tokio::pin!(servers);
    tokio::select! {
        result = &mut servers => {
            engine.save_queue();
            return result;
        }
        () = heron_service::shutdown_signal() => {}
    }
    match listener.take_standby() {
        Some(channel) => {
            let handing = std::sync::Arc::clone(&engine);
            let runtime = tokio::runtime::Handle::current();
            let handed = tokio::task::spawn_blocking(move || {
                handover::hand_over(&handing, channel, &runtime)
            })
            .await?;
            match handed {
                Ok(()) => tracing::info!("the new engine is on air"),
                Err(error) => {
                    tracing::error!(%error, "handover failed: stopping as usual");
                    engine.save_queue();
                }
            }
        }
        // Where the track on air is now, for the next start: first, before
        // anything can delay the exit.
        None => engine.save_queue(),
    }
    // Streams (the API's WatchEvents) never end on their own: do not wait
    // for them, or Docker kills the engine after 10 s.
    match tokio::time::timeout(SHUTDOWN_GRACE, &mut servers).await {
        Ok(result) => result,
        Err(_) => {
            tracing::info!("closing the connections still open");
            Ok(())
        }
    }
}
