// SPDX-License-Identifier: AGPL-3.0-or-later

//! Encoder profiles and outputs, stored in MariaDB and pushed to the engine
//! (see `sync`). See `docs/outputs.md`.
//!
//! Every change is checked as a whole configuration, the way the engine
//! would, before it is saved: an invalid change saves nothing.
//! Credentials in an output's settings (`password`) are never sent back;
//! an update without them keeps the stored ones. The statistics settings
//! (the stream server's admin credentials, `outputs.statistics`) are not
//! part of the engine's configuration: changing them does not reconnect.

use std::collections::HashMap;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use heron_core::{EncoderKind, EncoderProfile, OutputConfig, OutputKind, OutputsConfig};
use heron_outputs::icecast::IcecastConfig;
use serde::{Deserialize, Serialize};
use sqlx::{MySql, Transaction};
use utoipa::ToSchema;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::app::AppState;
use crate::auth::{CurrentUser, Role};
use crate::engine::OutputStatus;
use crate::error::{ApiError, ApiResult, ErrorBody, is_duplicate, is_still_used};
use crate::listeners::collector::StoredStatistics;
use crate::settings::some_or_null;

/// Settings never sent back to clients.
const SECRET_KEYS: [&str; 1] = ["password"];

#[derive(Clone, Debug, PartialEq, Serialize, ToSchema)]
pub struct EncoderProfileView {
    pub id: u64,
    pub name: String,
    pub encoder: EncoderKind,
    pub bitrate_kbps: Option<u16>,
    pub sample_rate: u32,
    pub channels: u8,
    /// Encoder-specific options, e.g. MP3: `{"mode": "cbr", "quality": 3}`.
    #[schema(value_type = Object)]
    pub options: serde_json::Value,
    /// Outputs using this profile.
    pub outputs: u32,
}

#[derive(Clone, Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct EncoderProfileInput {
    pub name: String,
    pub encoder: EncoderKind,
    pub bitrate_kbps: Option<u16>,
    pub sample_rate: u32,
    pub channels: u8,
    #[serde(default)]
    #[schema(value_type = Object)]
    pub options: serde_json::Value,
}

#[derive(Clone, Debug, PartialEq, Serialize, ToSchema)]
pub struct OutputView {
    pub id: u64,
    pub name: String,
    pub kind: OutputKind,
    pub encoder_profile_id: u64,
    /// Sink settings (Icecast: host, port, mount…), without credentials.
    #[schema(value_type = Object)]
    pub config: serde_json::Value,
    /// Credentials set in the settings (e.g. `password`), never sent back.
    pub secrets_set: Vec<String>,
    pub metadata_enabled: bool,
    pub enabled: bool,
    /// Where listeners can play the stream (Icecast).
    pub listen_url: Option<String>,
    /// Live state from the engine; missing while it is unreachable or
    /// before the output reaches it.
    pub status: Option<OutputStatus>,
    pub statistics: OutputStatisticsView,
}

/// What listener statistics read the stream server with.
#[derive(Clone, Debug, Default, PartialEq, Serialize, ToSchema)]
pub struct OutputStatisticsView {
    /// The server's admin user (`admin` when missing).
    pub admin_username: Option<String>,
    /// Whether its password is set (never sent back): without it, only
    /// concurrent listeners are counted.
    pub admin_password_set: bool,
}

impl From<StoredStatistics> for OutputStatisticsView {
    fn from(stored: StoredStatistics) -> Self {
        Self {
            admin_username: stored.admin_username,
            admin_password_set: stored.admin_password.is_some(),
        }
    }
}

/// Replaces the statistics settings of an output.
#[derive(Clone, Debug, Default, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct OutputStatisticsInput {
    /// The server's admin user; `admin` when missing.
    #[serde(default)]
    pub admin_username: Option<String>,
    /// Its password: left out, the stored one is kept; `null` removes it.
    #[serde(default, deserialize_with = "some_or_null")]
    #[schema(value_type = Option<String>)]
    pub admin_password: Option<Option<String>>,
}

impl OutputStatisticsInput {
    fn merge(self, stored: StoredStatistics) -> StoredStatistics {
        StoredStatistics {
            admin_username: self.admin_username.filter(|name| !name.trim().is_empty()),
            admin_password: match self.admin_password {
                None => stored.admin_password,
                Some(password) => password.filter(|password| !password.is_empty()),
            },
        }
    }
}

#[derive(Clone, Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct OutputInput {
    pub name: String,
    pub kind: OutputKind,
    pub encoder_profile_id: u64,
    /// Sink settings. Credentials left out keep their stored value.
    #[schema(value_type = Object)]
    pub config: serde_json::Value,
    #[serde(default = "yes")]
    pub metadata_enabled: bool,
    #[serde(default = "yes")]
    pub enabled: bool,
    /// The stream server's admin credentials, for listener statistics.
    /// Left out, the stored ones are kept.
    #[serde(default)]
    pub statistics: Option<OutputStatisticsInput>,
}

fn yes() -> bool {
    true
}

fn enum_text<T: Serialize>(value: T) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_default()
}

fn parse_enum<T: serde::de::DeserializeOwned>(text: String) -> ApiResult<T> {
    serde_json::from_value(serde_json::Value::String(text.clone()))
        .map_err(|_| ApiError::Internal(format!("unknown value {text:?} in the database")))
}

fn parse_json(text: Option<String>) -> serde_json::Value {
    text.and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or(serde_json::Value::Null)
}

type ProfileRow = (u64, String, String, Option<u16>, u32, u8, Option<String>);
type OutputRow = (u64, String, String, u64, String, bool, bool);

/// The whole configuration, in the shape the engine takes.
pub async fn load_config(executor: &mut sqlx::MySqlConnection) -> ApiResult<OutputsConfig> {
    let profiles: Vec<ProfileRow> = sqlx::query_as(
        "SELECT id, name, CAST(encoder AS CHAR), bitrate_kbps, sample_rate, channels,
                CAST(options AS CHAR)
         FROM encoder_profiles ORDER BY id",
    )
    .fetch_all(&mut *executor)
    .await?;
    let outputs: Vec<OutputRow> = sqlx::query_as(
        "SELECT id, name, CAST(kind AS CHAR), encoder_profile_id, CAST(config AS CHAR),
                metadata_enabled, enabled
         FROM outputs ORDER BY id",
    )
    .fetch_all(&mut *executor)
    .await?;
    let encoder_profiles = profiles
        .into_iter()
        .map(
            |(id, name, encoder, bitrate_kbps, sample_rate, channels, options)| {
                Ok(EncoderProfile {
                    id,
                    name,
                    encoder: parse_enum(encoder)?,
                    bitrate_kbps,
                    sample_rate,
                    channels,
                    options: match parse_json(options) {
                        serde_json::Value::Null => serde_json::json!({}),
                        options => options,
                    },
                })
            },
        )
        .collect::<ApiResult<Vec<_>>>()?;
    let outputs = outputs
        .into_iter()
        .map(
            |(id, name, kind, encoder_profile_id, config, metadata_enabled, enabled)| {
                Ok(OutputConfig {
                    id,
                    name,
                    kind: parse_enum(kind)?,
                    encoder_profile_id,
                    config: parse_json(Some(config)),
                    metadata_enabled,
                    enabled,
                })
            },
        )
        .collect::<ApiResult<Vec<_>>>()?;
    Ok(OutputsConfig {
        encoder_profiles,
        outputs,
    })
}

/// Checks the configuration as the engine will.
fn validate(config: &OutputsConfig) -> ApiResult<()> {
    heron_outputs::Outputs::validate(config).map_err(|error| ApiError::Invalid(error.to_string()))
}

/// Saves a change made in `tx` if the resulting configuration is valid,
/// and tells the engine.
async fn commit(state: &AppState, mut tx: Transaction<'_, MySql>) -> ApiResult<()> {
    let config = load_config(&mut tx).await?;
    validate(&config)?;
    tx.commit().await?;
    state.sync.changed();
    Ok(())
}

fn redact(config: &serde_json::Value) -> (serde_json::Value, Vec<String>) {
    let mut config = config.clone();
    let mut secrets = Vec::new();
    if let Some(object) = config.as_object_mut() {
        for key in SECRET_KEYS {
            if object.remove(key).is_some() {
                secrets.push(key.to_owned());
            }
        }
    }
    (config, secrets)
}

/// Keeps the stored credentials an update leaves out.
fn keep_secrets(new: &mut serde_json::Value, old: &serde_json::Value) {
    let (Some(new), Some(old)) = (new.as_object_mut(), old.as_object()) else {
        return;
    };
    for key in SECRET_KEYS {
        if !new.contains_key(key)
            && let Some(value) = old.get(key)
        {
            new.insert(key.to_owned(), value.clone());
        }
    }
}

fn output_view(
    output: OutputConfig,
    status: Option<OutputStatus>,
    statistics: StoredStatistics,
) -> OutputView {
    let listen_url = match output.kind {
        OutputKind::Icecast => IcecastConfig::parse(&output.name, &output.config)
            .ok()
            .map(|config| config.listen_url()),
        _ => None,
    };
    let (config, secrets_set) = redact(&output.config);
    OutputView {
        id: output.id,
        name: output.name,
        kind: output.kind,
        encoder_profile_id: output.encoder_profile_id,
        config,
        secrets_set,
        metadata_enabled: output.metadata_enabled,
        enabled: output.enabled,
        listen_url,
        status,
        statistics: statistics.into(),
    }
}

/// The statistics settings of every output.
async fn statistics(state: &AppState) -> ApiResult<HashMap<u64, StoredStatistics>> {
    let rows: Vec<(u64, Option<String>)> =
        sqlx::query_as("SELECT id, CAST(statistics AS CHAR) FROM outputs")
            .fetch_all(&state.pool)
            .await?;
    Ok(rows
        .into_iter()
        .map(|(id, json)| (id, StoredStatistics::parse(json.as_deref())))
        .collect())
}

fn statistics_json(statistics: &StoredStatistics) -> ApiResult<String> {
    serde_json::to_string(statistics).map_err(|error| ApiError::Internal(error.to_string()))
}

fn profile_view(profile: EncoderProfile, config: &OutputsConfig) -> EncoderProfileView {
    let outputs = config
        .outputs
        .iter()
        .filter(|output| output.encoder_profile_id == profile.id)
        .count();
    EncoderProfileView {
        id: profile.id,
        name: profile.name,
        encoder: profile.encoder,
        bitrate_kbps: profile.bitrate_kbps,
        sample_rate: profile.sample_rate,
        channels: profile.channels,
        options: profile.options,
        outputs: u32::try_from(outputs).unwrap_or(u32::MAX),
    }
}

fn check_name(name: &str) -> ApiResult<String> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > 255 {
        return Err(ApiError::Invalid(
            "names are 1 to 255 characters".to_owned(),
        ));
    }
    Ok(name.to_owned())
}

fn duplicate_or(error: sqlx::Error, what: &str) -> ApiError {
    if is_duplicate(&error) {
        ApiError::Conflict(format!("another {what} has this name"))
    } else {
        error.into()
    }
}

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_profiles, create_profile))
        .routes(routes!(get_profile, update_profile, delete_profile))
        .routes(routes!(list_outputs, create_output))
        .routes(routes!(get_output, update_output, delete_output))
}

async fn current(state: &AppState) -> ApiResult<OutputsConfig> {
    let mut connection = state.pool.acquire().await?;
    load_config(&mut connection).await
}

#[utoipa::path(get, path = "/encoder-profiles", tag = "outputs",
    responses((status = 200, body = Vec<EncoderProfileView>), (status = "4XX", body = ErrorBody)))]
async fn list_profiles(
    _user: CurrentUser,
    State(state): State<AppState>,
) -> ApiResult<Json<Vec<EncoderProfileView>>> {
    let config = current(&state).await?;
    let profiles = config
        .encoder_profiles
        .iter()
        .cloned()
        .map(|profile| profile_view(profile, &config))
        .collect();
    Ok(Json(profiles))
}

#[utoipa::path(get, path = "/encoder-profiles/{id}", tag = "outputs",
    params(("id" = u64, Path)),
    responses((status = 200, body = EncoderProfileView), (status = "4XX", body = ErrorBody)))]
async fn get_profile(
    _user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<u64>,
) -> ApiResult<Json<EncoderProfileView>> {
    let config = current(&state).await?;
    let profile = config
        .encoder_profiles
        .iter()
        .find(|profile| profile.id == id)
        .cloned()
        .ok_or_else(|| ApiError::not_found("encoder profile"))?;
    Ok(Json(profile_view(profile, &config)))
}

/// Adds an encoder profile. Admins only.
#[utoipa::path(post, path = "/encoder-profiles", tag = "outputs",
    request_body = EncoderProfileInput,
    responses((status = 201, body = EncoderProfileView), (status = "4XX", body = ErrorBody)))]
async fn create_profile(
    user: CurrentUser,
    State(state): State<AppState>,
    Json(input): Json<EncoderProfileInput>,
) -> ApiResult<(StatusCode, Json<EncoderProfileView>)> {
    user.require(Role::Admin)?;
    let name = check_name(&input.name)?;
    let mut tx = state.pool.begin().await?;
    let id = sqlx::query(
        "INSERT INTO encoder_profiles (name, encoder, bitrate_kbps, sample_rate, channels, options)
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(&name)
    .bind(enum_text(input.encoder))
    .bind(input.bitrate_kbps)
    .bind(input.sample_rate)
    .bind(input.channels)
    .bind(input.options.to_string())
    .execute(&mut *tx)
    .await
    .map_err(|error| duplicate_or(error, "encoder profile"))?
    .last_insert_id();
    commit(&state, tx).await?;
    tracing::info!(user = %user.username, id, %name, "encoder profile created");
    let Json(view) = get_profile(user, State(state), Path(id)).await?;
    Ok((StatusCode::CREATED, Json(view)))
}

/// Replaces an encoder profile. Outputs using it restart with the new
/// encoding. Admins only.
#[utoipa::path(put, path = "/encoder-profiles/{id}", tag = "outputs",
    params(("id" = u64, Path)), request_body = EncoderProfileInput,
    responses((status = 200, body = EncoderProfileView), (status = "4XX", body = ErrorBody)))]
async fn update_profile(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<u64>,
    Json(input): Json<EncoderProfileInput>,
) -> ApiResult<Json<EncoderProfileView>> {
    user.require(Role::Admin)?;
    let name = check_name(&input.name)?;
    let mut tx = state.pool.begin().await?;
    let updated = sqlx::query(
        "UPDATE encoder_profiles SET name = ?, encoder = ?, bitrate_kbps = ?, sample_rate = ?,
                channels = ?, options = ?
         WHERE id = ?",
    )
    .bind(&name)
    .bind(enum_text(input.encoder))
    .bind(input.bitrate_kbps)
    .bind(input.sample_rate)
    .bind(input.channels)
    .bind(input.options.to_string())
    .bind(id)
    .execute(&mut *tx)
    .await
    .map_err(|error| duplicate_or(error, "encoder profile"))?;
    if updated.rows_affected() == 0 && !row_exists(&mut tx, "encoder_profiles", id).await? {
        return Err(ApiError::not_found("encoder profile"));
    }
    commit(&state, tx).await?;
    tracing::info!(user = %user.username, id, %name, "encoder profile changed");
    get_profile(user, State(state), Path(id)).await
}

/// Deletes an encoder profile no output uses. Admins only.
#[utoipa::path(delete, path = "/encoder-profiles/{id}", tag = "outputs",
    params(("id" = u64, Path)),
    responses((status = 204), (status = "4XX", body = ErrorBody)))]
async fn delete_profile(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<u64>,
) -> ApiResult<StatusCode> {
    user.require(Role::Admin)?;
    let mut tx = state.pool.begin().await?;
    let deleted = sqlx::query("DELETE FROM encoder_profiles WHERE id = ?")
        .bind(id)
        .execute(&mut *tx)
        .await
        .map_err(|error| {
            if is_still_used(&error) {
                ApiError::Conflict("outputs still use this encoder profile".to_owned())
            } else {
                error.into()
            }
        })?;
    if deleted.rows_affected() == 0 {
        return Err(ApiError::not_found("encoder profile"));
    }
    commit(&state, tx).await?;
    tracing::info!(user = %user.username, id, "encoder profile deleted");
    Ok(StatusCode::NO_CONTENT)
}

fn statuses(state: &AppState) -> Vec<OutputStatus> {
    state
        .engine
        .state()
        .borrow()
        .as_ref()
        .map(|player| player.outputs.clone())
        .unwrap_or_default()
}

/// Outputs with their live state.
#[utoipa::path(get, path = "/outputs", tag = "outputs",
    responses((status = 200, body = Vec<OutputView>), (status = "4XX", body = ErrorBody)))]
async fn list_outputs(
    _user: CurrentUser,
    State(state): State<AppState>,
) -> ApiResult<Json<Vec<OutputView>>> {
    let config = current(&state).await?;
    let statuses = statuses(&state);
    let mut statistics = statistics(&state).await?;
    let outputs = config
        .outputs
        .into_iter()
        .map(|output| {
            let status = statuses.iter().find(|s| s.id == output.id).cloned();
            let stored = statistics.remove(&output.id).unwrap_or_default();
            output_view(output, status, stored)
        })
        .collect();
    Ok(Json(outputs))
}

#[utoipa::path(get, path = "/outputs/{id}", tag = "outputs",
    params(("id" = u64, Path)),
    responses((status = 200, body = OutputView), (status = "4XX", body = ErrorBody)))]
async fn get_output(
    _user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<u64>,
) -> ApiResult<Json<OutputView>> {
    let config = current(&state).await?;
    let output = config
        .outputs
        .into_iter()
        .find(|output| output.id == id)
        .ok_or_else(|| ApiError::not_found("output"))?;
    let status = statuses(&state).into_iter().find(|s| s.id == id);
    let stored = statistics(&state).await?.remove(&id).unwrap_or_default();
    Ok(Json(output_view(output, status, stored)))
}

/// Adds an output; the engine starts streaming to it if it is enabled.
/// Admins only.
#[utoipa::path(post, path = "/outputs", tag = "outputs",
    request_body = OutputInput,
    responses((status = 201, body = OutputView), (status = "4XX", body = ErrorBody)))]
async fn create_output(
    user: CurrentUser,
    State(state): State<AppState>,
    Json(input): Json<OutputInput>,
) -> ApiResult<(StatusCode, Json<OutputView>)> {
    user.require(Role::Admin)?;
    let name = check_name(&input.name)?;
    if !input.config.is_object() {
        return Err(ApiError::Invalid("config must be an object".to_owned()));
    }
    let statistics = input
        .statistics
        .unwrap_or_default()
        .merge(StoredStatistics::default());
    let mut tx = state.pool.begin().await?;
    let id = sqlx::query(
        "INSERT INTO outputs (name, kind, encoder_profile_id, config, statistics,
                              metadata_enabled, enabled)
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&name)
    .bind(enum_text(input.kind))
    .bind(input.encoder_profile_id)
    .bind(input.config.to_string())
    .bind(statistics_json(&statistics)?)
    .bind(input.metadata_enabled)
    .bind(input.enabled)
    .execute(&mut *tx)
    .await
    .map_err(|error| {
        if is_still_used(&error) {
            ApiError::Invalid(format!("no encoder profile {}", input.encoder_profile_id))
        } else {
            duplicate_or(error, "output")
        }
    })?
    .last_insert_id();
    commit(&state, tx).await?;
    tracing::info!(user = %user.username, id, %name, "output created");
    let Json(view) = get_output(user, State(state), Path(id)).await?;
    Ok((StatusCode::CREATED, Json(view)))
}

/// Replaces an output. Credentials left out of `config` are kept, and so
/// are the statistics settings when `statistics` is left out. Admins only.
#[utoipa::path(put, path = "/outputs/{id}", tag = "outputs",
    params(("id" = u64, Path)), request_body = OutputInput,
    responses((status = 200, body = OutputView), (status = "4XX", body = ErrorBody)))]
async fn update_output(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<u64>,
    Json(mut input): Json<OutputInput>,
) -> ApiResult<Json<OutputView>> {
    user.require(Role::Admin)?;
    let name = check_name(&input.name)?;
    if !input.config.is_object() {
        return Err(ApiError::Invalid("config must be an object".to_owned()));
    }
    let mut tx = state.pool.begin().await?;
    let stored: Option<(String, Option<String>)> = sqlx::query_as(
        "SELECT CAST(config AS CHAR), CAST(statistics AS CHAR) FROM outputs
         WHERE id = ? FOR UPDATE",
    )
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?;
    let Some((stored, stored_statistics)) = stored else {
        return Err(ApiError::not_found("output"));
    };
    keep_secrets(&mut input.config, &parse_json(Some(stored)));
    let stored_statistics = StoredStatistics::parse(stored_statistics.as_deref());
    let statistics = match input.statistics.take() {
        Some(statistics) => statistics.merge(stored_statistics),
        None => stored_statistics,
    };
    sqlx::query(
        "UPDATE outputs SET name = ?, kind = ?, encoder_profile_id = ?, config = ?,
                statistics = ?, metadata_enabled = ?, enabled = ?
         WHERE id = ?",
    )
    .bind(&name)
    .bind(enum_text(input.kind))
    .bind(input.encoder_profile_id)
    .bind(input.config.to_string())
    .bind(statistics_json(&statistics)?)
    .bind(input.metadata_enabled)
    .bind(input.enabled)
    .bind(id)
    .execute(&mut *tx)
    .await
    .map_err(|error| {
        if is_still_used(&error) {
            ApiError::Invalid(format!("no encoder profile {}", input.encoder_profile_id))
        } else {
            duplicate_or(error, "output")
        }
    })?;
    commit(&state, tx).await?;
    tracing::info!(user = %user.username, id, %name, "output changed");
    get_output(user, State(state), Path(id)).await
}

/// Deletes an output; the engine stops streaming to it. Admins only.
#[utoipa::path(delete, path = "/outputs/{id}", tag = "outputs",
    params(("id" = u64, Path)),
    responses((status = 204), (status = "4XX", body = ErrorBody)))]
async fn delete_output(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<u64>,
) -> ApiResult<StatusCode> {
    user.require(Role::Admin)?;
    let mut tx = state.pool.begin().await?;
    let deleted = sqlx::query("DELETE FROM outputs WHERE id = ?")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    if deleted.rows_affected() == 0 {
        return Err(ApiError::not_found("output"));
    }
    commit(&state, tx).await?;
    tracing::info!(user = %user.username, id, "output deleted");
    Ok(StatusCode::NO_CONTENT)
}

async fn row_exists(
    tx: &mut Transaction<'_, MySql>,
    table: &'static str,
    id: u64,
) -> ApiResult<bool> {
    let sql = format!("SELECT COUNT(*) FROM {table} WHERE id = ?");
    let count: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(sql))
        .bind(id)
        .fetch_one(&mut **tx)
        .await?;
    Ok(count > 0)
}

/// Copies the engine's own outputs into an empty database, the first time
/// the API manages the engine (stations set up with `heron-engine ctl`
/// before M4). Profile IDs are kept when they are free or identical, so
/// outputs keep streaming. Returns whether anything was copied.
pub async fn adopt(pool: &sqlx::MySqlPool, engine: &OutputsConfig) -> ApiResult<bool> {
    let mut tx = pool.begin().await?;
    let stored = load_config(&mut tx).await?;
    if !stored.outputs.is_empty() || engine.outputs.is_empty() {
        return Ok(false);
    }
    let mut profile_ids = std::collections::HashMap::new();
    for profile in &engine.encoder_profiles {
        let same = |stored: &EncoderProfile| {
            stored.name == profile.name
                && stored.encoder == profile.encoder
                && stored.bitrate_kbps == profile.bitrate_kbps
                && stored.sample_rate == profile.sample_rate
                && stored.channels == profile.channels
        };
        if let Some(existing) = stored.encoder_profiles.iter().find(|p| same(p)) {
            profile_ids.insert(profile.id, existing.id);
            continue;
        }
        let id_taken = stored.encoder_profiles.iter().any(|p| p.id == profile.id);
        let name_taken = stored
            .encoder_profiles
            .iter()
            .any(|p| p.name == profile.name);
        let name = if name_taken {
            format!("{} (engine)", profile.name)
        } else {
            profile.name.clone()
        };
        let id = sqlx::query(
            "INSERT INTO encoder_profiles (id, name, encoder, bitrate_kbps, sample_rate, channels, options)
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind((!id_taken).then_some(profile.id))
        .bind(name)
        .bind(enum_text(profile.encoder))
        .bind(profile.bitrate_kbps)
        .bind(profile.sample_rate)
        .bind(profile.channels)
        .bind(profile.options.to_string())
        .execute(&mut *tx)
        .await?
        .last_insert_id();
        profile_ids.insert(profile.id, id);
    }
    for output in &engine.outputs {
        let Some(profile_id) = profile_ids.get(&output.encoder_profile_id) else {
            continue;
        };
        sqlx::query(
            "INSERT INTO outputs (id, name, kind, encoder_profile_id, config, metadata_enabled, enabled)
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(output.id)
        .bind(&output.name)
        .bind(enum_text(output.kind))
        .bind(profile_id)
        .bind(output.config.to_string())
        .bind(output.metadata_enabled)
        .bind(output.enabled)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    tracing::info!(
        outputs = engine.outputs.len(),
        "copied the engine's outputs into the database"
    );
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn statistics_credentials_are_kept_or_removed() {
        let stored = StoredStatistics {
            admin_username: Some("ops".to_owned()),
            admin_password: Some("secret".to_owned()),
        };
        let input: OutputStatisticsInput =
            serde_json::from_str(r#"{"admin_username": "ops"}"#).unwrap();
        assert_eq!(input.merge(stored.clone()), stored);
        let input: OutputStatisticsInput =
            serde_json::from_str(r#"{"admin_password": null}"#).unwrap();
        assert_eq!(input.merge(stored.clone()), StoredStatistics::default());
        let input: OutputStatisticsInput =
            serde_json::from_str(r#"{"admin_username": "", "admin_password": "new"}"#).unwrap();
        let merged = input.merge(stored);
        assert_eq!(merged.admin_username, None);
        assert_eq!(merged.admin_password.as_deref(), Some("new"));
        let view = OutputStatisticsView::from(merged);
        assert!(view.admin_password_set);
        assert!(!serde_json::to_string(&view).unwrap().contains("new"));
    }

    #[test]
    fn credentials_are_hidden_and_kept() {
        let stored = serde_json::json!({"host": "icecast", "mount": "/live", "password": "hackme"});
        let (shown, secrets) = redact(&stored);
        assert_eq!(
            shown,
            serde_json::json!({"host": "icecast", "mount": "/live"})
        );
        assert_eq!(secrets, ["password"]);

        let mut update = serde_json::json!({"host": "other", "mount": "/live"});
        keep_secrets(&mut update, &stored);
        assert_eq!(update["password"], "hackme");
        let mut replaced = serde_json::json!({"host": "other", "password": "new"});
        keep_secrets(&mut replaced, &stored);
        assert_eq!(replaced["password"], "new");
    }

    #[test]
    fn enums_match_the_database() {
        assert_eq!(enum_text(EncoderKind::ExternalFfmpeg), "external_ffmpeg");
        assert_eq!(enum_text(OutputKind::ShoutcastV2), "shoutcast_v2");
        let kind: OutputKind = parse_enum("icecast".to_owned()).unwrap();
        assert_eq!(kind, OutputKind::Icecast);
    }
}
