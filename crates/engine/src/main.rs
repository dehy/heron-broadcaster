// SPDX-License-Identifier: AGPL-3.0-or-later

//! `heron-engine`: real-time playout (decks, mixer, DSP, encoders, outputs).
//!
//! In M0 it checks its directories and serves the ops endpoints. It never
//! connects to the database and must keep running when every other service is down.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::Context;
use clap::{Parser, Subcommand};
use heron_service::LogArgs;
use heron_service::ops::AlwaysHealthy;

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

    /// Directory of tracks played when the queue runs dry or the API is unreachable.
    #[arg(
        long,
        env = "HERON_FALLBACK_DIR",
        default_value = "/var/lib/heron/fallback"
    )]
    fallback_dir: PathBuf,

    /// Directory where the engine persists its state (queue, outputs) between restarts.
    #[arg(long, env = "HERON_STATE_DIR", default_value = "/var/lib/heron/engine")]
    state_dir: PathBuf,

    #[command(flatten)]
    log: LogArgs,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Exit 0 if the running engine answers `/healthz` (used by Docker).
    Healthcheck,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Some(Command::Healthcheck) => {
            heron_service::probe::probe(cli.http_addr, "/healthz", Duration::from_secs(3))?;
            Ok(())
        }
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
    tracing::info!(state_dir = %cli.state_dir.display(), "engine ready (playout arrives in M2)");

    let router = heron_service::ops_router(SERVICE, metrics, AlwaysHealthy);
    heron_service::serve(cli.http_addr, router).await?;
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
