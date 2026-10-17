// SPDX-License-Identifier: AGPL-3.0-or-later

//! Station settings, stored as JSON in the `settings` table, one row per group.
//!
//! - `playout`: crossfades and loudness, pushed to the engine.
//! - `analysis`: Gap Killer and crossfade point detection. Changing them
//!   re-analyzes the library.
//! - `library`: identification and layout, read by `heron-librarian`.
//! - `queue`: the queue manager.

use axum::Json;
use axum::extract::State;
use heron_core::{AnalysisSettings, LibrarySettings, PlayoutSettings};
use heron_library::layout::Layout;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use sqlx::MySqlPool;
use utoipa::ToSchema;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::app::AppState;
use crate::auth::{CurrentUser, Role};
use crate::error::{ApiError, ApiResult, ErrorBody};

/// The queue manager. Until rotation and clockwheels (M6), it keeps the
/// engine's queue filled with the least recently played tracks.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(default, deny_unknown_fields)]
pub struct QueueSettings {
    /// Add tracks when the queue runs low. Off, the station plays silence
    /// once the queue is empty.
    pub autofill: bool,
    /// Tracks kept waiting (cued and queued), 1 to 50.
    pub min_queued: u32,
}

impl QueueSettings {
    pub const SETTINGS_NAME: &'static str = "queue";
}

impl Default for QueueSettings {
    fn default() -> Self {
        Self {
            autofill: true,
            min_queued: 3,
        }
    }
}

/// Library settings as shown: the AcoustID key is never sent back.
#[derive(Clone, Debug, PartialEq, Serialize, ToSchema)]
pub struct LibrarySettingsView {
    /// Whether the station has set its AcoustID key (identification is off without one).
    pub acoustid_key_set: bool,
    pub layout: String,
    pub min_score: f64,
    pub save_cover: bool,
    pub embed_cover: bool,
}

impl From<LibrarySettings> for LibrarySettingsView {
    fn from(settings: LibrarySettings) -> Self {
        Self {
            acoustid_key_set: settings.acoustid_key.is_some(),
            layout: settings.layout,
            min_score: settings.min_score,
            save_cover: settings.save_cover,
            embed_cover: settings.embed_cover,
        }
    }
}

/// Changes to the library settings: missing fields are kept.
#[derive(Clone, Debug, Default, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct LibrarySettingsPatch {
    /// The station's AcoustID application key; `null` removes it.
    #[serde(default, deserialize_with = "some_or_null")]
    #[schema(value_type = Option<String>)]
    pub acoustid_key: Option<Option<String>>,
    /// See `docs/library.md#layout`.
    pub layout: Option<String>,
    /// Lowest AcoustID score (0 to 1) accepted as a match.
    pub min_score: Option<f64>,
    pub save_cover: Option<bool>,
    pub embed_cover: Option<bool>,
}

/// Tells a field set to `null` (`Some(None)`) from a missing one (`None`).
fn some_or_null<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<Option<String>>, D::Error> {
    Option::<String>::deserialize(deserializer).map(Some)
}

/// Marks the engine's configuration as managed by the API (see `sync`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct EngineManagement {
    pub managed: bool,
}

impl EngineManagement {
    pub const SETTINGS_NAME: &'static str = "engine";
}

/// Reads a settings group; its defaults when it was never saved.
pub async fn load<T: DeserializeOwned + Default>(
    executor: impl sqlx::MySqlExecutor<'_>,
    name: &str,
) -> ApiResult<T> {
    let value: Option<String> =
        sqlx::query_scalar("SELECT CAST(value AS CHAR) FROM settings WHERE name = ?")
            .bind(name)
            .fetch_optional(executor)
            .await?;
    match value {
        None => Ok(T::default()),
        Some(json) => serde_json::from_str(&json)
            .map_err(|error| ApiError::Internal(format!("stored {name} settings: {error}"))),
    }
}

/// Whether a settings group was ever saved.
pub async fn exists(pool: &MySqlPool, name: &str) -> ApiResult<bool> {
    Ok(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM settings WHERE name = ?")
            .bind(name)
            .fetch_one(pool)
            .await?
            > 0,
    )
}

pub async fn save<T: Serialize>(
    executor: impl sqlx::MySqlExecutor<'_>,
    name: &str,
    value: &T,
) -> ApiResult<()> {
    let json = serde_json::to_string(value)
        .map_err(|error| ApiError::Internal(format!("{name} settings: {error}")))?;
    sqlx::query(
        "INSERT INTO settings (name, value) VALUES (?, ?)
         ON DUPLICATE KEY UPDATE value = VALUES(value)",
    )
    .bind(name)
    .bind(json)
    .execute(executor)
    .await?;
    Ok(())
}

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(get_playout, put_playout))
        .routes(routes!(get_analysis, put_analysis))
        .routes(routes!(get_library, patch_library))
        .routes(routes!(get_queue_settings, put_queue_settings))
}

/// Crossfades and loudness normalization.
#[utoipa::path(get, path = "/settings/playout", tag = "settings",
    responses((status = 200, body = PlayoutSettings), (status = "4XX", body = ErrorBody)))]
async fn get_playout(
    _user: CurrentUser,
    State(state): State<AppState>,
) -> ApiResult<Json<PlayoutSettings>> {
    Ok(Json(
        load(&state.pool, PlayoutSettings::SETTINGS_NAME).await?,
    ))
}

/// Replaces the playout settings (missing fields take the defaults) and
/// pushes them to the engine. Admins only.
#[utoipa::path(put, path = "/settings/playout", tag = "settings",
    request_body = PlayoutSettings,
    responses((status = 200, body = PlayoutSettings), (status = "4XX", body = ErrorBody)))]
async fn put_playout(
    user: CurrentUser,
    State(state): State<AppState>,
    Json(settings): Json<PlayoutSettings>,
) -> ApiResult<Json<PlayoutSettings>> {
    user.require(Role::Admin)?;
    check_playout(&settings)?;
    save(&state.pool, PlayoutSettings::SETTINGS_NAME, &settings).await?;
    tracing::info!(user = %user.username, "playout settings changed");
    state.sync.changed();
    Ok(Json(settings))
}

fn check_playout(settings: &PlayoutSettings) -> ApiResult<()> {
    let crossfade = &settings.crossfade;
    for (name, fade) in [
        ("fade_out", crossfade.fade_out),
        ("fade_in", crossfade.fade_in),
    ] {
        if fade.level_pct > 100 {
            return Err(ApiError::Invalid(format!(
                "{name}.level_pct is a percentage, 0 to 100"
            )));
        }
        if fade.time_ms > 60_000 {
            return Err(ApiError::Invalid(format!(
                "{name}.time_ms is at most 60,000"
            )));
        }
    }
    let loudness = &settings.loudness;
    if !(-70.0..=0.0).contains(&loudness.target_lufs) {
        return Err(ApiError::Invalid(
            "loudness.target_lufs must be between -70 and 0".to_owned(),
        ));
    }
    if !(0.0..=40.0).contains(&loudness.max_gain_db) {
        return Err(ApiError::Invalid(
            "loudness.max_gain_db must be between 0 and 40".to_owned(),
        ));
    }
    Ok(())
}

/// Gap Killer and crossfade point detection, used at import.
#[utoipa::path(get, path = "/settings/analysis", tag = "settings",
    responses((status = 200, body = AnalysisSettings), (status = "4XX", body = ErrorBody)))]
async fn get_analysis(
    _user: CurrentUser,
    State(state): State<AppState>,
) -> ApiResult<Json<AnalysisSettings>> {
    Ok(Json(
        load(&state.pool, AnalysisSettings::SETTINGS_NAME).await?,
    ))
}

/// Replaces the analysis settings. Every track is analyzed again with the
/// new settings, in the background. Admins only.
#[utoipa::path(put, path = "/settings/analysis", tag = "settings",
    request_body = AnalysisSettings,
    responses((status = 200, body = AnalysisSettings), (status = "4XX", body = ErrorBody)))]
async fn put_analysis(
    user: CurrentUser,
    State(state): State<AppState>,
    Json(settings): Json<AnalysisSettings>,
) -> ApiResult<Json<AnalysisSettings>> {
    user.require(Role::Admin)?;
    if settings.fade_min_ms > settings.fade_max_ms {
        return Err(ApiError::Invalid(
            "fade_min_ms must not be above fade_max_ms".to_owned(),
        ));
    }
    let previous: AnalysisSettings = load(&state.pool, AnalysisSettings::SETTINGS_NAME).await?;
    save(&state.pool, AnalysisSettings::SETTINGS_NAME, &settings).await?;
    if previous != settings {
        let queued = heron_analysis::jobs::enqueue_stale(&state.pool, &settings)
            .await
            .map_err(|error| ApiError::Internal(error.to_string()))?;
        tracing::info!(user = %user.username, queued, "analysis settings changed: re-analyzing");
    }
    Ok(Json(settings))
}

/// Identification and folder layout, used by `heron-librarian`.
#[utoipa::path(get, path = "/settings/library", tag = "settings",
    responses((status = 200, body = LibrarySettingsView), (status = "4XX", body = ErrorBody)))]
async fn get_library(
    _user: CurrentUser,
    State(state): State<AppState>,
) -> ApiResult<Json<LibrarySettingsView>> {
    let settings: LibrarySettings = load(&state.pool, LibrarySettings::SETTINGS_NAME).await?;
    Ok(Json(settings.into()))
}

/// Changes some library settings; the others are kept. Admins only.
#[utoipa::path(patch, path = "/settings/library", tag = "settings",
    request_body = LibrarySettingsPatch,
    responses((status = 200, body = LibrarySettingsView), (status = "4XX", body = ErrorBody)))]
async fn patch_library(
    user: CurrentUser,
    State(state): State<AppState>,
    Json(patch): Json<LibrarySettingsPatch>,
) -> ApiResult<Json<LibrarySettingsView>> {
    user.require(Role::Admin)?;
    let mut settings: LibrarySettings = load(&state.pool, LibrarySettings::SETTINGS_NAME).await?;
    if let Some(key) = patch.acoustid_key {
        settings.acoustid_key = key
            .map(|key| key.trim().to_owned())
            .filter(|key| !key.is_empty());
    }
    if let Some(layout) = patch.layout {
        Layout::parse(&layout).map_err(|error| ApiError::Invalid(format!("layout: {error}")))?;
        settings.layout = layout;
    }
    if let Some(min_score) = patch.min_score {
        if !(0.0..=1.0).contains(&min_score) {
            return Err(ApiError::Invalid("min_score is between 0 and 1".to_owned()));
        }
        settings.min_score = min_score;
    }
    settings.save_cover = patch.save_cover.unwrap_or(settings.save_cover);
    settings.embed_cover = patch.embed_cover.unwrap_or(settings.embed_cover);
    save(&state.pool, LibrarySettings::SETTINGS_NAME, &settings).await?;
    tracing::info!(user = %user.username, ?settings, "library settings changed");
    Ok(Json(settings.into()))
}

#[utoipa::path(get, path = "/settings/queue", tag = "settings",
    responses((status = 200, body = QueueSettings), (status = "4XX", body = ErrorBody)))]
async fn get_queue_settings(
    _user: CurrentUser,
    State(state): State<AppState>,
) -> ApiResult<Json<QueueSettings>> {
    Ok(Json(load(&state.pool, QueueSettings::SETTINGS_NAME).await?))
}

/// Replaces the queue manager settings. Operators and admins.
#[utoipa::path(put, path = "/settings/queue", tag = "settings",
    request_body = QueueSettings,
    responses((status = 200, body = QueueSettings), (status = "4XX", body = ErrorBody)))]
async fn put_queue_settings(
    user: CurrentUser,
    State(state): State<AppState>,
    Json(settings): Json<QueueSettings>,
) -> ApiResult<Json<QueueSettings>> {
    user.require(Role::Operator)?;
    if !(1..=50).contains(&settings.min_queued) {
        return Err(ApiError::Invalid(
            "min_queued is between 1 and 50".to_owned(),
        ));
    }
    save(&state.pool, QueueSettings::SETTINGS_NAME, &settings).await?;
    state.queue.wake();
    Ok(Json(settings))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn library_patch_tells_null_from_missing() {
        let clear: LibrarySettingsPatch =
            serde_json::from_str(r#"{"acoustid_key": null}"#).unwrap();
        assert_eq!(clear.acoustid_key, Some(None));
        let keep: LibrarySettingsPatch =
            serde_json::from_str(r#"{"layout": "{title}.{ext}"}"#).unwrap();
        assert_eq!(keep.acoustid_key, None);
        let set: LibrarySettingsPatch = serde_json::from_str(r#"{"acoustid_key": "k"}"#).unwrap();
        assert_eq!(set.acoustid_key, Some(Some("k".to_owned())));
    }

    #[test]
    fn rejects_odd_playout_settings() {
        let mut settings = PlayoutSettings::default();
        check_playout(&settings).unwrap();
        settings.crossfade.fade_out.level_pct = 150;
        assert!(check_playout(&settings).is_err());
    }
}
