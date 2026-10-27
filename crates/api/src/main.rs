// SPDX-License-Identifier: AGPL-3.0-or-later

//! `heron-api`: HTTP and WebSocket API, library scanner and queue manager.
//!
//! It applies the database migrations on startup, scans the music library,
//! feeds the engine and serves the API and the ops endpoints. See
//! `docs/api.md`.

mod app;
mod artists;
mod auth;
mod categories;
mod category_types;
mod clocks;
#[cfg(test)]
mod e2e;
mod engine;
mod error;
mod history;
mod import;
mod labels;
mod librarian;
mod library;
mod listeners;
mod live;
mod medleys;
mod outputs;
mod player;
mod public;
mod queue;
mod restrictions;
mod rotation;
mod selection;
mod settings;
mod staging;
mod sync;
mod tracks;
mod users;
mod webhooks;

use std::io::{BufRead, IsTerminal, Write};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use app::{AppState, SERVICE};
use auth::{Passwords, Role};
use clap::{Parser, Subcommand};
use heron_service::db::{self, DatabaseHealth, MIGRATOR};
use heron_service::{LogArgs, MetricsArgs};
use library::{Library, ScanOptions};

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
    #[arg(
        long,
        env = "HERON_DATABASE_URL",
        hide_env_values = true,
        global = true
    )]
    database_url: Option<String>,

    /// gRPC address of the engine.
    #[arg(
        long,
        env = "HERON_ENGINE_URL",
        default_value = "http://heron-engine:50051"
    )]
    engine_url: String,

    /// Root of the music library. Track paths are stored relative to it.
    #[arg(long, env = "HERON_LIBRARY_DIR", default_value = "/music")]
    library_dir: PathBuf,

    /// The buffer zone, where uploads are written (read-write, shared with
    /// heron-librarian, which imports them).
    #[arg(long, env = "HERON_STAGING_DIR", default_value = "/staging")]
    staging_dir: PathBuf,

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

    #[command(flatten)]
    metrics: MetricsArgs,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Exit 0 if the running server answers `/healthz` (used by Docker).
    Healthcheck,
    /// Print the OpenAPI document (JSON).
    Openapi,
    /// Manage users. Passwords are read from the terminal, or from the
    /// first line of stdin when it is not a terminal.
    #[command(subcommand)]
    User(UserCommand),
}

#[derive(Debug, Subcommand)]
enum UserCommand {
    /// Create a user, e.g. the first admin: `user add admin --role admin`.
    Add {
        username: String,
        /// viewer, operator or admin.
        #[arg(long, default_value = "viewer")]
        role: Role,
    },
    /// Set a user's password (and sign them out everywhere).
    Passwd { username: String },
    /// List users.
    List,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Some(Command::Healthcheck) => {
            heron_service::probe::probe(cli.http_addr, "/healthz", Duration::from_secs(3))?;
            Ok(())
        }
        Some(Command::Openapi) => {
            let document = serde_json::to_string_pretty(&app::openapi())?;
            println!("{document}");
            Ok(())
        }
        Some(Command::User(command)) => manage_users(cli.database_url, command),
        None => run(cli),
    }
}

#[tokio::main]
async fn run(cli: Cli) -> anyhow::Result<()> {
    let _telemetry = heron_service::telemetry::init_tracing(&cli.log, SERVICE)?;
    let metrics = heron_service::ops::install_metrics(SERVICE, &cli.metrics)?;
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
    let users: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE enabled")
        .fetch_one(&pool)
        .await?;
    if users == 0 {
        tracing::warn!(
            "no user yet: create the first admin with \
             `docker compose exec heron-api heron-api user add admin --role admin`"
        );
    }

    let library = Arc::new(Library::new(pool.clone(), cli.library_dir));
    let engine = engine::EngineLink::new(&cli.engine_url)
        .with_context(|| format!("invalid engine URL {}", cli.engine_url))?;
    let queue = queue::QueueManager::new(pool.clone(), Arc::clone(&engine));
    let sync = sync::ConfigSync::new(pool.clone(), Arc::clone(&engine));

    let mut tasks = engine.spawn();
    let history_events = engine.events();
    let webhook_events = engine.events();
    let webhooks = webhooks::Webhooks::new(pool.clone()).context("cannot set up webhooks")?;
    let listeners =
        listeners::Collector::new(pool.clone()).context("cannot set up listener statistics")?;
    tasks.push(tokio::spawn(library.clone().run(ScanOptions {
        interval: Duration::from_secs(cli.scan_interval.max(60)),
        watch: cli.library_watch,
        settle: Duration::from_secs(cli.import_settle),
    })));
    tasks.push(tokio::spawn(
        history::HistoryRecorder::new(pool.clone()).run(history_events),
    ));
    tasks.push(tokio::spawn(Arc::clone(&webhooks).dispatch(webhook_events)));
    tasks.push(tokio::spawn(
        Arc::clone(&webhooks).watch_queue(engine.state()),
    ));
    tasks.push(tokio::spawn(Arc::clone(&webhooks).send_loop()));
    tasks.push(tokio::spawn(Arc::clone(&listeners).run()));
    tasks.push(tokio::spawn(Arc::clone(&queue).run()));
    tasks.push(tokio::spawn(artists::run_linker(
        pool.clone(),
        Arc::clone(&queue),
    )));
    tasks.push(tokio::spawn(medleys::run_linker(pool.clone())));
    tasks.push(tokio::spawn(Arc::clone(&sync).run()));
    tasks.push(tokio::spawn(users::purge_sessions(pool.clone())));

    let health = DatabaseHealth(pool.clone());
    let state = AppState {
        pool: pool.clone(),
        health: health.clone(),
        library,
        engine,
        queue,
        sync,
        passwords: Arc::new(Passwords::default()),
        staging: Arc::new(cli.staging_dir),
        webhooks,
        listeners,
    };
    let router = app::router(state).merge(heron_service::ops_router(SERVICE, metrics, health));

    let served = heron_service::serve(cli.http_addr, router).await;
    for task in tasks {
        task.abort();
    }
    pool.close().await;
    Ok(served?)
}

/// Reads a password without echoing it on a terminal.
fn read_password(prompt: &str) -> anyhow::Result<String> {
    let stdin = std::io::stdin();
    let terminal = stdin.is_terminal();
    let mut saved = None;
    if terminal {
        eprint!("{prompt}");
        std::io::stderr().flush()?;
        if let Ok(mut termios) = rustix::termios::tcgetattr(&stdin) {
            saved = Some(termios.clone());
            termios
                .local_modes
                .remove(rustix::termios::LocalModes::ECHO);
            let _ =
                rustix::termios::tcsetattr(&stdin, rustix::termios::OptionalActions::Now, &termios);
        }
    }
    let mut line = String::new();
    let read = stdin.lock().read_line(&mut line);
    if let Some(saved) = saved {
        let _ = rustix::termios::tcsetattr(&stdin, rustix::termios::OptionalActions::Now, &saved);
        eprintln!();
    }
    read?;
    Ok(line.trim_end_matches(['\r', '\n']).to_owned())
}

#[tokio::main(flavor = "current_thread")]
async fn manage_users(database_url: Option<String>, command: UserCommand) -> anyhow::Result<()> {
    let database_url =
        database_url.context("HERON_DATABASE_URL (or --database-url) is required")?;
    let pool = db::connect(
        &database_url,
        db::ConnectOptions {
            max_connections: 1,
            max_attempts: 3,
        },
    )
    .await?;
    MIGRATOR
        .run(&pool)
        .await
        .context("failed to apply database migrations")?;
    let passwords = Passwords::default();
    match command {
        UserCommand::Add { username, role } => {
            let password = read_password("Password: ")?;
            let user = users::add_user(&pool, &passwords, &username, &password, role)
                .await
                .map_err(|error| anyhow::anyhow!("{error}"))?;
            println!("user {} created ({})", user.username, user.role.as_str());
        }
        UserCommand::Passwd { username } => {
            let id = users::user_id(&pool, &username)
                .await
                .map_err(|error| anyhow::anyhow!("{error}"))?;
            let password = read_password("New password: ")?;
            users::set_password(&pool, &passwords, id, &password, None)
                .await
                .map_err(|error| anyhow::anyhow!("{error}"))?;
            println!("password of {username} changed");
        }
        UserCommand::List => {
            let users = users::all_users(&pool)
                .await
                .map_err(|error| anyhow::anyhow!("{error}"))?;
            for user in users {
                println!(
                    "{:<24} {:<9} {}",
                    user.username,
                    user.role.as_str(),
                    if user.enabled { "" } else { "disabled" }
                );
            }
        }
    }
    pool.close().await;
    Ok(())
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
            "artist_names",
            "artist_similar",
            "artist_suggestions",
            "artists",
            "categories",
            "clock_schedule",
            "clock_slots",
            "clocks",
            "encoder_profiles",
            "listener_day_keys",
            "listener_samples",
            "listener_sessions",
            "listener_totals",
            "outputs",
            "play_history",
            "restrictions",
            "rotation_picks",
            "sessions",
            "settings",
            "track_analysis",
            "track_artists",
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
