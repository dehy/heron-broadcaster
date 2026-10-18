// SPDX-License-Identifier: AGPL-3.0-or-later

//! The ops endpoints every binary exposes: `/healthz` and `/metrics`.

use std::future::Future;

use axum::Router;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};
use serde_json::json;

use crate::Error;

/// Something `/healthz` asks before answering.
pub trait HealthCheck: Clone + Send + Sync + 'static {
    /// Returns `Err` with a short reason when the service is not healthy.
    fn check(&self) -> impl Future<Output = Result<(), String>> + Send;

    /// Problems worth an operator's attention that do not make the service
    /// unhealthy (an unsynchronized clock). Listed in `/healthz` as `warnings`.
    fn warnings(&self) -> impl Future<Output = Vec<String>> + Send {
        async { Vec::new() }
    }
}

/// A service with no dependencies to check: healthy as long as it answers.
#[derive(Clone, Copy, Debug, Default)]
pub struct AlwaysHealthy;

impl HealthCheck for AlwaysHealthy {
    async fn check(&self) -> Result<(), String> {
        Ok(())
    }
}

/// Installs the Prometheus recorder, records `heron_build_info` and, when
/// configured, starts pushing the metrics with remote write (a bad remote
/// write configuration is logged, not fatal). Call it from the Tokio runtime.
pub fn install_metrics(
    service: &'static str,
    remote_write: &crate::MetricsArgs,
) -> Result<PrometheusHandle, Error> {
    let handle = PrometheusBuilder::new().install_recorder()?;
    // Monitoring must never keep a service (the engine above all) from running.
    if let Err(error) = crate::remote_write::spawn(handle.clone(), service, remote_write) {
        tracing::error!(%error, "metrics are not pushed");
    }
    metrics::gauge!(
        "heron_build_info",
        "service" => service,
        "version" => env!("CARGO_PKG_VERSION"),
    )
    .set(1.0);
    Ok(handle)
}

#[derive(Clone)]
struct OpsState<H> {
    service: &'static str,
    metrics: PrometheusHandle,
    health: H,
}

/// Router serving `/healthz` and `/metrics`. Merge it into the service's own router.
pub fn ops_router<H: HealthCheck>(
    service: &'static str,
    metrics: PrometheusHandle,
    health: H,
) -> Router {
    Router::new()
        .route("/healthz", get(healthz::<H>))
        .route("/metrics", get(render_metrics::<H>))
        .with_state(OpsState {
            service,
            metrics,
            health,
        })
}

async fn healthz<H: HealthCheck>(State(state): State<OpsState<H>>) -> Response {
    let body = |status: &str| {
        json!({
            "service": state.service,
            "version": env!("CARGO_PKG_VERSION"),
            "status": status,
        })
    };
    let (status, mut payload) = match state.health.check().await {
        Ok(()) => (StatusCode::OK, body("ok")),
        Err(reason) => {
            let mut payload = body("unavailable");
            payload["reason"] = reason.into();
            (StatusCode::SERVICE_UNAVAILABLE, payload)
        }
    };
    let warnings = state.health.warnings().await;
    if !warnings.is_empty() {
        payload["warnings"] = warnings.into();
    }
    (status, axum::Json(payload)).into_response()
}

async fn render_metrics<H: HealthCheck>(State(state): State<OpsState<H>>) -> impl IntoResponse {
    state.metrics.render()
}
