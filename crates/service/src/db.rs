// SPDX-License-Identifier: AGPL-3.0-or-later

//! MariaDB connection helpers. Never used by `heron-engine`.

use std::time::Duration;

use sqlx::mysql::{MySqlPool, MySqlPoolOptions};

use crate::{Error, HealthCheck};

/// How long to keep retrying the first connection, and how many connections to open.
#[derive(Clone, Copy, Debug)]
pub struct ConnectOptions {
    pub max_connections: u32,
    pub max_attempts: u32,
}

impl Default for ConnectOptions {
    fn default() -> Self {
        Self {
            max_connections: 10,
            max_attempts: 30,
        }
    }
}

/// Connects to MariaDB, retrying with exponential backoff (capped at 10 s)
/// so services can start before the database is ready.
///
/// The URL is never logged: it contains the password.
pub async fn connect(url: &str, options: ConnectOptions) -> Result<MySqlPool, Error> {
    let mut delay = Duration::from_millis(500);
    let mut attempt = 1;
    loop {
        let result = MySqlPoolOptions::new()
            .max_connections(options.max_connections)
            .acquire_timeout(Duration::from_secs(5))
            .connect(url)
            .await;
        match result {
            Ok(pool) => {
                tracing::info!(attempt, "connected to the database");
                return Ok(pool);
            }
            Err(source) if attempt >= options.max_attempts => {
                return Err(Error::Database {
                    attempts: attempt,
                    source,
                });
            }
            Err(error) => {
                tracing::warn!(attempt, %error, retry_in = ?delay, "database not reachable yet");
                tokio::time::sleep(delay).await;
                delay = (delay * 2).min(Duration::from_secs(10));
                attempt += 1;
            }
        }
    }
}

/// Health check that pings the database.
#[derive(Clone, Debug)]
pub struct DatabaseHealth(pub MySqlPool);

impl HealthCheck for DatabaseHealth {
    async fn check(&self) -> Result<(), String> {
        sqlx::query("SELECT 1")
            .execute(&self.0)
            .await
            .map(|_| ())
            .map_err(|error| format!("database: {error}"))
    }
}
