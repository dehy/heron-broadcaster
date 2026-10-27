// SPDX-License-Identifier: AGPL-3.0-or-later

//! The deck EQ: its settings (the music EQ of Decks A and B, the FX EQ),
//! pushed to the engine as soon as they change, and its presets. Presets
//! stay here: loading one is the client copying its gains into an EQ. See
//! `docs/api.md#deck-eq`.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use heron_core::{EQ_BANDS, EqSettings, eq::check_gains};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::app::AppState;
use crate::auth::{CurrentUser, Role};
use crate::error::{ApiError, ApiResult, ErrorBody, is_duplicate};
use crate::settings;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(get_eq, put_eq))
        .routes(routes!(list_presets, create_preset))
        .routes(routes!(update_preset, delete_preset))
}

/// Both deck EQs.
#[utoipa::path(get, path = "/settings/eq", tag = "settings",
    responses((status = 200, body = EqSettings), (status = "4XX", body = ErrorBody)))]
async fn get_eq(_user: CurrentUser, State(state): State<AppState>) -> ApiResult<Json<EqSettings>> {
    Ok(Json(
        settings::load(&state.pool, EqSettings::SETTINGS_NAME).await?,
    ))
}

/// Replaces both deck EQs and pushes them to the engine at once, where
/// they glide in. Every open live WebSocket gets an `eq` message.
/// Operators and admins.
#[utoipa::path(put, path = "/settings/eq", tag = "settings",
    request_body = EqSettings,
    responses((status = 200, body = EqSettings), (status = "4XX", body = ErrorBody)))]
async fn put_eq(
    user: CurrentUser,
    State(state): State<AppState>,
    Json(eq): Json<EqSettings>,
) -> ApiResult<Json<EqSettings>> {
    user.require(Role::Operator)?;
    eq.check().map_err(ApiError::Invalid)?;
    settings::save(&state.pool, EqSettings::SETTINGS_NAME, &eq).await?;
    // Nobody listening is fine.
    let _ = state.eq_changes.send(eq);
    let json = serde_json::to_string(&eq).map_err(|error| ApiError::Internal(error.to_string()))?;
    // Saved either way: the configuration sync pushes it when the engine
    // is back, and reports a refusal.
    match state.engine.set_eq_settings(json).await {
        Ok(()) | Err(ApiError::EngineUnavailable) => {}
        Err(error) => {
            tracing::warn!(%error, "the engine refused the deck EQ");
            state.sync.changed();
        }
    }
    Ok(Json(eq))
}

#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct EqPreset {
    pub id: u64,
    pub name: String,
    /// Gain of each band in dB, from 60 Hz up.
    pub gains_db: [f32; EQ_BANDS],
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct EqPresetInput {
    /// Unique, ignoring case.
    pub name: String,
    /// Gain of each band in dB, from 60 Hz up, −12 to +12.
    pub gains_db: [f32; EQ_BANDS],
}

impl EqPresetInput {
    fn check(&self) -> ApiResult<(String, String)> {
        let name = self.name.trim();
        if name.is_empty() || name.chars().count() > 100 {
            return Err(ApiError::Invalid(
                "preset names are 1 to 100 characters".to_owned(),
            ));
        }
        check_gains(&self.gains_db).map_err(ApiError::Invalid)?;
        let gains = serde_json::to_string(&self.gains_db)
            .map_err(|error| ApiError::Internal(error.to_string()))?;
        Ok((name.to_owned(), gains))
    }
}

#[derive(sqlx::FromRow)]
struct PresetRow {
    id: u64,
    name: String,
    gains_db: String,
}

impl TryFrom<PresetRow> for EqPreset {
    type Error = ApiError;

    fn try_from(row: PresetRow) -> ApiResult<Self> {
        let gains_db = serde_json::from_str(&row.gains_db)
            .map_err(|error| ApiError::Internal(format!("stored EQ preset {}: {error}", row.id)))?;
        Ok(Self {
            id: row.id,
            name: row.name,
            gains_db,
        })
    }
}

const SELECT: &str = "SELECT id, name, CAST(gains_db AS CHAR) AS gains_db FROM eq_presets";

fn conflict(error: sqlx::Error) -> ApiError {
    if is_duplicate(&error) {
        ApiError::Conflict("another preset has this name".to_owned())
    } else {
        error.into()
    }
}

async fn find(state: &AppState, id: u64) -> ApiResult<EqPreset> {
    sqlx::query_as::<_, PresetRow>(sqlx::AssertSqlSafe(format!("{SELECT} WHERE id = ?")))
        .bind(id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| ApiError::not_found("EQ preset"))?
        .try_into()
}

/// Every EQ preset, by name.
#[utoipa::path(get, path = "/eq-presets", tag = "settings",
    responses((status = 200, body = Vec<EqPreset>), (status = "4XX", body = ErrorBody)))]
async fn list_presets(
    _user: CurrentUser,
    State(state): State<AppState>,
) -> ApiResult<Json<Vec<EqPreset>>> {
    let rows: Vec<PresetRow> =
        sqlx::query_as(sqlx::AssertSqlSafe(format!("{SELECT} ORDER BY name")))
            .fetch_all(&state.pool)
            .await?;
    Ok(Json(
        rows.into_iter()
            .map(EqPreset::try_from)
            .collect::<ApiResult<_>>()?,
    ))
}

/// Saves gains as a new preset. Operators and admins.
#[utoipa::path(post, path = "/eq-presets", tag = "settings",
    request_body = EqPresetInput,
    responses((status = 201, body = EqPreset), (status = "4XX", body = ErrorBody)))]
async fn create_preset(
    user: CurrentUser,
    State(state): State<AppState>,
    Json(input): Json<EqPresetInput>,
) -> ApiResult<(StatusCode, Json<EqPreset>)> {
    user.require(Role::Operator)?;
    let (name, gains) = input.check()?;
    let id = sqlx::query("INSERT INTO eq_presets (name, gains_db) VALUES (?, ?)")
        .bind(name)
        .bind(gains)
        .execute(&state.pool)
        .await
        .map_err(conflict)?
        .last_insert_id();
    Ok((StatusCode::CREATED, Json(find(&state, id).await?)))
}

/// Renames a preset or saves new gains in it. The EQs it was loaded into
/// keep their gains. Operators and admins.
#[utoipa::path(put, path = "/eq-presets/{id}", tag = "settings",
    params(("id" = u64, Path)), request_body = EqPresetInput,
    responses((status = 200, body = EqPreset), (status = "4XX", body = ErrorBody)))]
async fn update_preset(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<u64>,
    Json(input): Json<EqPresetInput>,
) -> ApiResult<Json<EqPreset>> {
    user.require(Role::Operator)?;
    let (name, gains) = input.check()?;
    sqlx::query("UPDATE eq_presets SET name = ?, gains_db = ? WHERE id = ?")
        .bind(name)
        .bind(gains)
        .bind(id)
        .execute(&state.pool)
        .await
        .map_err(conflict)?;
    Ok(Json(find(&state, id).await?))
}

/// Deletes a preset. The EQs it was loaded into keep their gains.
/// Operators and admins.
#[utoipa::path(delete, path = "/eq-presets/{id}", tag = "settings",
    params(("id" = u64, Path)),
    responses((status = 204), (status = "4XX", body = ErrorBody)))]
async fn delete_preset(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<u64>,
) -> ApiResult<StatusCode> {
    user.require(Role::Operator)?;
    let deleted = sqlx::query("DELETE FROM eq_presets WHERE id = ?")
        .bind(id)
        .execute(&state.pool)
        .await?;
    if deleted.rows_affected() == 0 {
        return Err(ApiError::not_found("EQ preset"));
    }
    Ok(StatusCode::NO_CONTENT)
}
