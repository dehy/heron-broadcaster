// SPDX-License-Identifier: AGPL-3.0-or-later

//! `heron-engine`: real-time playout (decks, mixer, encoders, outputs).
//!
//! It never connects to the database and must keep running when every other
//! service is down. `heron-engine ctl` is a command-line client for its gRPC
//! control interface.

mod ctl;

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::Context;
use clap::{Parser, Subcommand};
use heron_engine::engine::{Engine, EngineHealth, EngineOptions};
use heron_engine::grpc::EngineService;
use heron_service::LogArgs;

const SERVICE: &str = "heron-engine";

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

    /// Play the master bus on the local sound card (builds with the `monitor` feature).
    #[arg(long, env = "HERON_MONITOR")]
    monitor: bool,

    /// Directory where the engine persists its state (configuration, queue) between restarts.
    #[arg(long, env = "HERON_STATE_DIR", default_value = "/var/lib/heron/engine")]
    state_dir: PathBuf,

    #[command(flatten)]
    log: LogArgs,
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
    heron_service::telemetry::init_tracing(&cli.log)?;
    let metrics = heron_service::ops::install_metrics(SERVICE)?;

    std::fs::create_dir_all(&cli.state_dir).with_context(|| {
        format!(
            "cannot create the state directory {}",
            cli.state_dir.display()
        )
    })?;
    check_fallback_dir(&cli.fallback_dir);

    let engine = Engine::start(
        EngineOptions {
            library_dir: cli.library_dir.clone(),
            state_dir: cli.state_dir.clone(),
            monitor: cli.monitor,
        },
        tokio::runtime::Handle::current(),
    )?;
    tracing::info!(
        library_dir = %cli.library_dir.display(),
        state_dir = %cli.state_dir.display(),
        "engine ready"
    );

    let grpc = tonic::transport::Server::builder()
        .add_service(EngineService::server(std::sync::Arc::clone(&engine)))
        .serve_with_shutdown(cli.grpc_addr, heron_service::shutdown_signal());
    tracing::info!(addr = %cli.grpc_addr, "grpc server listening");

    let health = EngineHealth(engine.status_handle());
    let router = heron_service::ops_router(SERVICE, metrics, health);
    let (grpc, http) = tokio::join!(grpc, heron_service::serve(cli.http_addr, router));
    grpc.context("grpc server failed")?;
    http?;
    Ok(())
}

/// A missing fallback directory is not fatal, but it removes a safety net, so say so loudly.
fn check_fallback_dir(dir: &Path) {
    match std::fs::read_dir(dir) {
        Ok(entries) => {
            let files = entries.filter_map(Result::ok).count();
            if files == 0 {
                tracing::warn!(dir = %dir.display(), "fallback directory is empty");
            } else {
                tracing::info!(dir = %dir.display(), files, "fallback directory found");
            }
        }
        Err(error) => {
            tracing::warn!(dir = %dir.display(), %error, "fallback directory is not readable");
        }
    }
}
