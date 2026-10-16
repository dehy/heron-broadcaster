// SPDX-License-Identifier: AGPL-3.0-or-later

//! Structured logging with `tracing`.

use tracing_subscriber::EnvFilter;

use crate::Error;

/// Log output format.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, clap::ValueEnum)]
pub enum LogFormat {
    /// Human-readable lines, for local development.
    #[default]
    Text,
    /// One JSON object per line, for containers and log collectors.
    Json,
}

/// Logging options shared by every binary.
#[derive(Clone, Debug, clap::Args)]
pub struct LogArgs {
    /// Log output format. The level is set with `RUST_LOG` (default `info`).
    #[arg(long, env = "HERON_LOG_FORMAT", value_enum, default_value_t = LogFormat::Text, global = true)]
    pub log_format: LogFormat,
}

/// Installs the global `tracing` subscriber.
pub fn init_tracing(args: &LogArgs) -> Result<(), Error> {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let builder = tracing_subscriber::fmt().with_env_filter(filter);
    match args.log_format {
        LogFormat::Text => builder.try_init(),
        LogFormat::Json => builder.json().flatten_event(true).try_init(),
    }
    .map_err(|error| Error::Tracing(error.to_string()))
}
