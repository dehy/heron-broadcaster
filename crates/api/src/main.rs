// SPDX-License-Identifier: AGPL-3.0-or-later

//! `heron-api`: HTTP and WebSocket API, library scanner and queue manager.
//!
//! In M0 it connects to MariaDB, applies the migrations and serves the ops
//! endpoints plus `GET /api/v1/status`.

use std::net::SocketAddr;
use std::time::Duration;

use anyhow::Context;
use axum::Router;
use axum::extract::State;
use axum::routing::get;
use clap::{Parser, Subcommand};
use heron_service::db::{self, DatabaseHealth};
use heron_service::{HealthCheck, LogArgs};
use serde::Serialize;

const SERVICE: &str = "heron-api";

static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("../../migrations");

#[derive(Debug, Parser)]
#[command(name = SERVICE, version, about = "Heron Broadcaster API")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,

    /// Address of the HTTP server (API, `/healthz`, `/metrics`).
    #[arg(
        long,
        env = "HERON_HTTP_ADDR",
        default_value = "0.0.0.0:8080",
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
    /// Exit 0 if the running server answers `/healthz` (used by Docker).
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
    MIGRATOR
        .run(&pool)
        .await
        .context("failed to apply database migrations")?;
    tracing::info!(
        migrations = MIGRATOR.iter().count(),
        "database schema is up to date"
    );

    let health = DatabaseHealth(pool.clone());
    let router = Router::new()
        .route("/api/v1/status", get(status))
        .with_state(AppState {
            health: health.clone(),
        })
        .merge(heron_service::ops_router(SERVICE, metrics, health));

    heron_service::serve(cli.http_addr, router).await?;
    pool.close().await;
    Ok(())
}

#[derive(Clone)]
struct AppState {
    health: DatabaseHealth,
}

#[derive(Serialize)]
struct Status {
    service: &'static str,
    version: &'static str,
    database: &'static str,
}

async fn status(State(state): State<AppState>) -> axum::Json<Status> {
    let database = match state.health.check().await {
        Ok(()) => "ok",
        Err(_) => "unavailable",
    };
    axum::Json(Status {
        service: SERVICE,
        version: env!("CARGO_PKG_VERSION"),
        database,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Applies the migrations to the MariaDB in `HERON_TEST_DATABASE_URL`
    /// (an empty database). Skipped when the variable is not set; CI sets it.
    #[tokio::test]
    async fn migrations_apply_and_are_idempotent() {
        let Ok(url) = std::env::var("HERON_TEST_DATABASE_URL") else {
            eprintln!("HERON_TEST_DATABASE_URL not set, skipping");
            return;
        };
        let pool = db::connect(&url, db::ConnectOptions::default())
            .await
            .unwrap();
        MIGRATOR.run(&pool).await.unwrap();
        MIGRATOR.run(&pool).await.unwrap();

        let tables: Vec<String> = sqlx::query_scalar(
            "SELECT CAST(table_name AS CHAR) FROM information_schema.tables \
             WHERE table_schema = DATABASE() ORDER BY table_name",
        )
        .fetch_all(&pool)
        .await
        .unwrap();
        for expected in [
            "analysis_jobs",
            "api_tokens",
            "categories",
            "clock_schedule",
            "clock_slots",
            "clocks",
            "encoder_profiles",
            "outputs",
            "play_history",
            "sessions",
            "settings",
            "track_analysis",
            "track_categories",
            "track_overrides",
            "tracks",
            "users",
            "webhook_deliveries",
            "webhooks",
        ] {
            assert!(tables.iter().any(|t| t == expected), "missing {expected}");
        }

        // Long paths stay unique through the generated hash column. Rolled
        // back so the test can run again on the same database.
        let mut tx = pool.begin().await.unwrap();
        let long_path = format!("/music/{}.flac", "a".repeat(3000));
        sqlx::query("INSERT INTO tracks (path) VALUES (?)")
            .bind(&long_path)
            .execute(&mut *tx)
            .await
            .unwrap();
        let duplicate = sqlx::query("INSERT INTO tracks (path) VALUES (?)")
            .bind(&long_path)
            .execute(&mut *tx)
            .await;
        assert!(duplicate.is_err());
        tx.rollback().await.unwrap();
    }
}
