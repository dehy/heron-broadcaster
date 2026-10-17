// SPDX-License-Identifier: AGPL-3.0-or-later

//! The HTTP API: shared state, routes and the OpenAPI document.
//!
//! Every route lives under `/api/v1`. The OpenAPI document is generated
//! from the handlers and served at `/api/v1/openapi.json`; the web client's
//! types are generated from it (`web/src/api/schema.ts`).

use std::sync::Arc;

use axum::Json;
use axum::Router;
use axum::extract::State;
use heron_service::HealthCheck;
use heron_service::db::DatabaseHealth;
use serde::Serialize;
use sqlx::MySqlPool;
use utoipa::openapi::security::{ApiKey, ApiKeyValue, HttpAuthScheme, HttpBuilder, SecurityScheme};
use utoipa::{Modify, OpenApi, ToSchema};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::auth::{Passwords, SESSION_COOKIE};
use crate::engine::EngineLink;
use crate::error::{ApiError, ApiResult, ErrorBody};
use crate::library::{Library, LibraryStatus};
use crate::queue::QueueManager;
use crate::sync::ConfigSync;
use crate::{categories, history, live, outputs, player, settings, tracks, users};

pub const SERVICE: &str = "heron-api";

#[derive(Clone)]
pub struct AppState {
    pub pool: MySqlPool,
    pub health: DatabaseHealth,
    pub library: Arc<Library>,
    pub engine: Arc<EngineLink>,
    pub queue: Arc<QueueManager>,
    pub sync: Arc<ConfigSync>,
    pub passwords: Arc<Passwords>,
}

#[derive(OpenApi)]
#[openapi(
    info(
        title = "Heron Broadcaster API",
        description = "Control plane of Heron Broadcaster. Sign in with `POST /api/v1/auth/login` \
                       (session cookie, plus an `X-Requested-With` header on requests that change \
                       something) or send an API token as `Authorization: Bearer heron_…`.",
        license(name = "AGPL-3.0-or-later", identifier = "AGPL-3.0-or-later"),
    ),
    modifiers(&Security),
    security(("session" = []), ("token" = [])),
    components(schemas(ErrorBody)),
    tags(
        (name = "system", description = "Status and the OpenAPI document"),
        (name = "auth", description = "Signing in, your account and your API tokens"),
        (name = "users", description = "User management (admins)"),
        (name = "player", description = "Decks, transport, FX player and the engine"),
        (name = "queue", description = "The tracks waiting to play"),
        (name = "library", description = "Tracks, their analysis and overrides"),
        (name = "categories", description = "Groups of tracks"),
        (name = "history", description = "What was played"),
        (name = "settings", description = "Station settings"),
        (name = "outputs", description = "Encoder profiles and outputs"),
        (name = "live", description = "Live state over a WebSocket"),
    )
)]
struct ApiDoc;

struct Security;

impl Modify for Security {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        let components = openapi.components.get_or_insert_with(Default::default);
        components.add_security_scheme(
            "session",
            SecurityScheme::ApiKey(ApiKey::Cookie(ApiKeyValue::new(SESSION_COOKIE))),
        );
        components.add_security_scheme(
            "token",
            SecurityScheme::Http(HttpBuilder::new().scheme(HttpAuthScheme::Bearer).build()),
        );
    }
}

fn api() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(status))
        .routes(routes!(library_status))
        .merge(users::router())
        .merge(player::router())
        .merge(tracks::router())
        .merge(categories::router())
        .merge(history::router())
        .merge(settings::router())
        .merge(outputs::router())
        .merge(live::router())
}

/// The OpenAPI document, as served.
pub fn openapi() -> utoipa::openapi::OpenApi {
    let (_, mut openapi) = OpenApiRouter::<AppState>::with_openapi(ApiDoc::openapi())
        .nest("/api/v1", api())
        .split_for_parts();
    openapi.info.version = env!("CARGO_PKG_VERSION").to_owned();
    openapi
}

pub fn router(state: AppState) -> Router {
    let (router, _) = OpenApiRouter::with_openapi(ApiDoc::openapi())
        .nest("/api/v1", api())
        .split_for_parts();
    let document = Arc::new(openapi());
    router
        .route(
            "/api/v1/openapi.json",
            axum::routing::get(move || {
                let document = Arc::clone(&document);
                async move { Json(document.as_ref().clone()) }
            }),
        )
        .with_state(state)
}

#[derive(Serialize, ToSchema)]
pub struct Status {
    pub service: &'static str,
    pub version: &'static str,
    /// `ok` or `unavailable`.
    pub database: &'static str,
    /// `ok` or `unavailable`.
    pub engine: &'static str,
}

/// Whether the API, its database and the engine are up. No sign-in needed.
#[utoipa::path(get, path = "/status", tag = "system", security(()),
    responses((status = 200, body = Status)))]
async fn status(State(state): State<AppState>) -> Json<Status> {
    let database = match state.health.check().await {
        Ok(()) => "ok",
        Err(_) => "unavailable",
    };
    Json(Status {
        service: SERVICE,
        version: env!("CARGO_PKG_VERSION"),
        database,
        engine: if state.engine.is_connected() {
            "ok"
        } else {
            "unavailable"
        },
    })
}

/// Import and analysis progress. No sign-in needed.
#[utoipa::path(get, path = "/library/status", tag = "library", security(()),
    responses((status = 200, body = LibraryStatus), (status = "5XX", body = ErrorBody)))]
async fn library_status(State(state): State<AppState>) -> ApiResult<Json<LibraryStatus>> {
    state.library.status().await.map(Json).map_err(|error| {
        tracing::warn!(%error, "cannot read the library status");
        ApiError::Internal(error.to_string())
    })
}
