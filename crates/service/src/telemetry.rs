// SPDX-License-Identifier: AGPL-3.0-or-later

//! Structured logging with `tracing`, and error reporting to Sentry.
//!
//! Sentry is off unless `HERON_SENTRY_DSN` is set. When it is on, errors
//! logged with `tracing::error!` and panics are reported as Sentry events,
//! with the warnings and informational logs before them as breadcrumbs.
//! Nothing is sent from the audio thread, which never logs.

use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Layer};

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

/// Logging and error reporting options shared by every binary.
#[derive(Clone, Debug, clap::Args)]
pub struct LogArgs {
    /// Log output format. The level is set with `RUST_LOG` (default `info`).
    #[arg(long, env = "HERON_LOG_FORMAT", value_enum, default_value_t = LogFormat::Text, global = true)]
    pub log_format: LogFormat,

    /// Sentry DSN: errors and panics are reported to this Sentry project.
    /// Off when empty.
    #[arg(long, env = "HERON_SENTRY_DSN", global = true, hide_env_values = true)]
    pub sentry_dsn: Option<String>,

    /// Environment reported to Sentry (production, staging…).
    #[arg(
        long,
        env = "HERON_SENTRY_ENVIRONMENT",
        default_value = "production",
        global = true
    )]
    pub sentry_environment: String,

    /// Share of requests traced for Sentry's performance monitoring, from 0
    /// (none) to 1 (all).
    #[arg(
        long,
        env = "HERON_SENTRY_TRACES_SAMPLE_RATE",
        default_value_t = 0.0,
        global = true
    )]
    pub sentry_traces_sample_rate: f32,
}

/// Keep it alive until the process exits: dropping it flushes the events
/// not sent yet.
#[must_use = "dropping it stops error reporting"]
pub struct Telemetry {
    _sentry: Option<sentry::ClientInitGuard>,
}

/// Installs the global `tracing` subscriber, and Sentry when configured.
pub fn init_tracing(args: &LogArgs, service: &'static str) -> Result<Telemetry, Error> {
    // A bad DSN is logged once logging works, never fatal: error reporting
    // must not keep a service (the engine above all) from running.
    let mut invalid_dsn = None;
    let sentry = args
        .sentry_dsn
        .as_deref()
        .map(str::trim)
        .filter(|dsn| !dsn.is_empty())
        .and_then(|dsn| match dsn.parse::<sentry::types::Dsn>() {
            Ok(dsn) => Some(dsn),
            Err(error) => {
                invalid_dsn = Some(error.to_string());
                None
            }
        })
        .map(|dsn| {
            let mut options = sentry::ClientOptions::new()
                .release(format!("{service}@{}", env!("CARGO_PKG_VERSION")))
                .environment(args.sentry_environment.clone())
                // Never IP addresses or request bodies.
                .send_default_pii(false);
            options.dsn = Some(dsn);
            let rate = args.sentry_traces_sample_rate;
            if rate > 0.0 {
                options = options.traces_sample_rate(rate.min(1.0));
            }
            let guard = sentry::init(options);
            sentry::configure_scope(|scope| scope.set_tag("service", service));
            guard
        });

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let output = match args.log_format {
        LogFormat::Text => tracing_subscriber::fmt::layer().boxed(),
        LogFormat::Json => tracing_subscriber::fmt::layer()
            .json()
            .flatten_event(true)
            .boxed(),
    };
    tracing_subscriber::registry()
        .with(filter)
        .with(output)
        .with(sentry.is_some().then(sentry::integrations::tracing::layer))
        .try_init()
        .map_err(|error| Error::Tracing(error.to_string()))?;
    if sentry.is_some() {
        tracing::info!(environment = %args.sentry_environment, "reporting errors to Sentry");
    }
    if let Some(error) = invalid_dsn {
        tracing::error!(%error, "invalid HERON_SENTRY_DSN: errors are not reported to Sentry");
    }
    Ok(Telemetry { _sentry: sentry })
}
