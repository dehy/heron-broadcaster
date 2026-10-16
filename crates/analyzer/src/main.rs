// SPDX-License-Identifier: AGPL-3.0-or-later

//! `heron-analyzer`: track analysis worker pool.
//!
//! In M0 it connects to MariaDB and serves the ops endpoints. Consuming
//! analysis jobs comes in M1. It never applies migrations: `heron-api` owns the schema.

use std::net::SocketAddr;
use std::time::Duration;

use anyhow::Context;
use clap::{Parser, Subcommand};
use heron_service::LogArgs;
use heron_service::db::{self, DatabaseHealth};

const SERVICE: &str = "heron-analyzer";

#[derive(Debug, Parser)]
#[command(name = SERVICE, version, about = "Heron Broadcaster analysis worker")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,

    /// Address of the ops HTTP server (`/healthz`, `/metrics`).
    #[arg(
        long,
        env = "HERON_HTTP_ADDR",
        default_value = "0.0.0.0:8082",
        global = true
    )]
    http_addr: SocketAddr,

    /// MariaDB URL, e.g. `mysql://heron:secret@mariadb:3306/heron`.
    #[arg(long, env = "HERON_DATABASE_URL", hide_env_values = true)]
    database_url: Option<String>,

    #[command(flatten)]
    log: LogArgs,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Exit 0 if the running worker answers `/healthz` (used by Docker).
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
    let database_url = cli
        .database_url
        .context("HERON_DATABASE_URL (or --database-url) is required")?;

    let pool = db::connect(&database_url, db::ConnectOptions::default()).await?;
    tracing::info!("analysis worker ready (job processing arrives in M1)");

    let router = heron_service::ops_router(SERVICE, metrics, DatabaseHealth(pool.clone()));
    heron_service::serve(cli.http_addr, router).await?;
    pool.close().await;
    Ok(())
}
