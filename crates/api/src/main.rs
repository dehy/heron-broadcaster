// SPDX-License-Identifier: AGPL-3.0-or-later

//! `heron-api`: HTTP and WebSocket API, library scanner and queue manager.
//!
//! It applies the database migrations on startup, scans the music library
//! and serves the API and the ops endpoints.

mod library;

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use axum::Router;
use axum::extract::State;
use axum::routing::get;
use clap::{Parser, Subcommand};
use heron_service::db::{self, DatabaseHealth, MIGRATOR};
use heron_service::{HealthCheck, LogArgs};
use library::{Library, ScanOptions};
use serde::Serialize;

const SERVICE: &str = "heron-api";

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

    /// Root of the music library. Track paths are stored relative to it.
    #[arg(long, env = "HERON_LIBRARY_DIR", default_value = "/music")]
    library_dir: PathBuf,

    /// Seconds between full library scans (file system events also trigger scans).
    #[arg(long, env = "HERON_SCAN_INTERVAL", default_value_t = 3600)]
    scan_interval: u64,

    /// Watch the library for changes. Turn off on file systems without
    /// change notifications (some network shares); periodic scans still run.
    #[arg(long, env = "HERON_LIBRARY_WATCH", default_value_t = true, action = clap::ArgAction::Set)]
    library_watch: bool,

    /// Seconds a file must stay unmodified before it is imported, so files
    /// still being copied are not read half-written.
    #[arg(long, env = "HERON_IMPORT_SETTLE", default_value_t = 5)]
    import_settle: u64,

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

    let library = Arc::new(Library::new(pool.clone(), cli.library_dir));
    let scanner = tokio::spawn(library.clone().run(ScanOptions {
        interval: Duration::from_secs(cli.scan_interval.max(60)),
        watch: cli.library_watch,
        settle: Duration::from_secs(cli.import_settle),
    }));

    let health = DatabaseHealth(pool.clone());
    let router = Router::new()
        .route("/api/v1/status", get(status))
        .route("/api/v1/library/status", get(library_status))
        .with_state(AppState {
            health: health.clone(),
            library,
        })
        .merge(heron_service::ops_router(SERVICE, metrics, health));

    let served = heron_service::serve(cli.http_addr, router).await;
    scanner.abort();
    pool.close().await;
    Ok(served?)
}

#[derive(Clone)]
struct AppState {
    health: DatabaseHealth,
    library: Arc<Library>,
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

async fn library_status(
    State(state): State<AppState>,
) -> Result<axum::Json<library::LibraryStatus>, (axum::http::StatusCode, String)> {
    state
        .library
        .status()
        .await
        .map(axum::Json)
        .map_err(|error| {
            tracing::warn!(%error, "cannot read the library status");
            (
                axum::http::StatusCode::SERVICE_UNAVAILABLE,
                "library status unavailable".to_owned(),
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    use heron_service::db::ScratchDatabase;

    /// Applies the migrations to a scratch MariaDB database (see
    /// `HERON_TEST_DATABASE_URL`). Skipped when no test database is configured.
    #[tokio::test]
    async fn migrations_apply_and_are_idempotent() {
        let Some(db) = ScratchDatabase::create().await else {
            return;
        };
        let pool = db.pool.clone();
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

        // The default encoder profile (docs/outputs.md#default-profiles).
        let (name, encoder, bitrate, rate, channels, mode): (String, String, u16, u32, u8, String) =
            sqlx::query_as(
                "SELECT name, CAST(encoder AS CHAR), bitrate_kbps, sample_rate, channels, \
                 JSON_UNQUOTE(JSON_EXTRACT(options, '$.mode')) FROM encoder_profiles",
            )
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(
            (
                name.as_str(),
                encoder.as_str(),
                bitrate,
                rate,
                channels,
                mode.as_str()
            ),
            ("MP3 128k", "mp3", 128, 44_100, 2, "cbr")
        );

        // Long paths stay unique through the generated hash column.
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
        db.drop().await;
    }
}
