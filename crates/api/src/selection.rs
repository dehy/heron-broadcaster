// SPDX-License-Identifier: AGPL-3.0-or-later

//! Actions on a selection of tracks, given by IDs or by the filters of the
//! track list (every matching track, on every page): labels, categories,
//! mood, sound code and tempo, and the files' tags (written by
//! `heron-librarian`, as a library task).

use axum::Json;
use axum::extract::State;
use heron_core::library::{FieldValues, TagField, TaskRequest};
use serde::{Deserialize, Serialize};
use sqlx::QueryBuilder;
use utoipa::ToSchema;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::app::AppState;
use crate::auth::{CurrentUser, Role};
use crate::error::{ApiError, ApiResult, ErrorBody};
use crate::librarian::{self, LibraryTask};
use crate::settings::{self, ProgrammingSettings, some_or_null};
use crate::tracks::{ListContext, TrackQuery};

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(apply_labels))
        .routes(routes!(apply_categories))
        .routes(routes!(apply_programming))
        .routes(routes!(edit_tags))
}

/// Tracks in one selection, at most.
pub const MAX_SELECTION: usize = 100_000;
/// Files in one tag edit, at most.
pub const MAX_TAG_EDIT: usize = 10_000;
/// IDs per statement.
const CHUNK: usize = 1_000;

/// Some tracks: these IDs, or every track matching the filters.
#[derive(Clone, Debug, Default, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct TrackSelection {
    pub track_ids: Option<Vec<u64>>,
    /// The filters of `GET /tracks` (sorting and pages aside).
    pub filters: Option<TrackQuery>,
}

impl TrackSelection {
    /// The selected tracks that exist, at most `max`.
    pub async fn resolve(&self, state: &AppState, max: usize) -> ApiResult<Vec<u64>> {
        match (&self.track_ids, &self.filters) {
            (Some(ids), None) => {
                if ids.len() > max {
                    return Err(ApiError::Invalid(format!(
                        "the selection has more than {max} tracks"
                    )));
                }
                let mut existing = Vec::with_capacity(ids.len());
                for chunk in ids.chunks(CHUNK) {
                    let mut query = QueryBuilder::new("SELECT id FROM tracks WHERE id IN (");
                    let mut separated = query.separated(", ");
                    for id in chunk {
                        separated.push_bind(*id);
                    }
                    query.push(")");
                    existing.extend(
                        query
                            .build_query_scalar::<u64>()
                            .fetch_all(&state.pool)
                            .await?,
                    );
                }
                existing.sort_unstable();
                existing.dedup();
                Ok(existing)
            }
            (None, Some(filters)) => {
                ListContext::load(&state.pool)
                    .await?
                    .ids(&state.pool, filters, max)
                    .await
            }
            _ => Err(ApiError::BadRequest(
                "select tracks with either track_ids or filters".to_owned(),
            )),
        }
    }
}

/// What an action on a selection did.
#[derive(Debug, Serialize, ToSchema)]
pub struct SelectionResult {
    /// Tracks selected.
    pub tracks: u64,
    /// Changes made (a label added to a track counts once).
    pub changed: u64,
}

/// Refuses labels that do not exist.
pub async fn check_labels(pool: &sqlx::MySqlPool, add: &[u64], remove: &[u64]) -> ApiResult<()> {
    for &id in add.iter().chain(remove) {
        let exists: Option<u64> = sqlx::query_scalar("SELECT id FROM labels WHERE id = ?")
            .bind(id)
            .fetch_optional(pool)
            .await?;
        if exists.is_none() {
            return Err(ApiError::Invalid(format!("no label {id}")));
        }
    }
    Ok(())
}

/// Refuses categories that do not exist, and weights that are not above 0.
pub async fn check_categories(
    pool: &sqlx::MySqlPool,
    add: &[CategoryShare],
    remove: &[u64],
) -> ApiResult<()> {
    if add
        .iter()
        .any(|share| !share.weight.is_finite() || share.weight <= 0.0)
    {
        return Err(ApiError::Invalid("weights must be above 0".to_owned()));
    }
    for id in add
        .iter()
        .map(|share| share.category_id)
        .chain(remove.iter().copied())
    {
        let exists: Option<u64> = sqlx::query_scalar("SELECT id FROM categories WHERE id = ?")
            .bind(id)
            .fetch_optional(pool)
            .await?;
        if exists.is_none() {
            return Err(ApiError::Invalid(format!("no category {id}")));
        }
    }
    Ok(())
}

/// Refuses a mood or a sound code that is not on the station's scale.
pub async fn check_scales(
    pool: &sqlx::MySqlPool,
    mood: &Option<Option<String>>,
    sound_code: &Option<Option<String>>,
) -> ApiResult<()> {
    let settings: ProgrammingSettings =
        settings::load(pool, ProgrammingSettings::SETTINGS_NAME).await?;
    let on_scale = |what: &str, value: &Option<Option<String>>, scale: &[String]| match value {
        Some(Some(value)) if !scale.contains(value) => Err(ApiError::Invalid(format!(
            "{value:?} is not on the {what} scale"
        ))),
        _ => Ok(()),
    };
    on_scale("mood", mood, &settings.moods)?;
    on_scale("sound code", sound_code, &settings.sound_codes)
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct LabelsChange {
    pub selection: TrackSelection,
    /// Labels added to every selected track.
    #[serde(default)]
    pub add: Vec<u64>,
    /// Labels removed from every selected track.
    #[serde(default)]
    pub remove: Vec<u64>,
}

/// Adds labels to, and removes labels from, every selected track.
/// Operators and admins.
#[utoipa::path(post, path = "/tracks/labels", tag = "labels",
    request_body = LabelsChange,
    responses((status = 200, body = SelectionResult), (status = "4XX", body = ErrorBody)))]
async fn apply_labels(
    user: CurrentUser,
    State(state): State<AppState>,
    Json(change): Json<LabelsChange>,
) -> ApiResult<Json<SelectionResult>> {
    user.require(Role::Operator)?;
    if change.add.is_empty() && change.remove.is_empty() {
        return Err(ApiError::Invalid("no label to add or remove".to_owned()));
    }
    if change.add.iter().any(|id| change.remove.contains(id)) {
        return Err(ApiError::Invalid(
            "a label cannot be added and removed at once".to_owned(),
        ));
    }
    check_labels(&state.pool, &change.add, &change.remove).await?;
    let ids = change.selection.resolve(&state, MAX_SELECTION).await?;
    let mut tx = state.pool.begin().await?;
    let mut changed = 0;
    for chunk in ids.chunks(CHUNK) {
        for &label in &change.add {
            let mut query =
                QueryBuilder::new("INSERT IGNORE INTO track_labels (track_id, label_id) ");
            query.push_values(chunk, |mut row, id| {
                row.push_bind(*id).push_bind(label);
            });
            changed += query.build().execute(&mut *tx).await?.rows_affected();
        }
        if !change.remove.is_empty() {
            let mut query = QueryBuilder::new("DELETE FROM track_labels WHERE label_id IN (");
            let mut separated = query.separated(", ");
            for label in &change.remove {
                separated.push_bind(*label);
            }
            query.push(") AND track_id IN (");
            let mut separated = query.separated(", ");
            for id in chunk {
                separated.push_bind(*id);
            }
            query.push(")");
            changed += query.build().execute(&mut *tx).await?.rows_affected();
        }
    }
    tx.commit().await?;
    tracing::info!(user = %user.username, tracks = ids.len(), changed, "labels changed");
    Ok(Json(SelectionResult {
        tracks: ids.len() as u64,
        changed,
    }))
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CategoryShare {
    pub category_id: u64,
    /// Relative chance of being picked within the category, 1 by default.
    #[serde(default = "one")]
    pub weight: f64,
}

fn one() -> f64 {
    1.0
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CategoriesChange {
    pub selection: TrackSelection,
    /// Categories every selected track is put in (with this weight, also
    /// for tracks already in them).
    #[serde(default)]
    pub add: Vec<CategoryShare>,
    /// Categories every selected track is taken out of.
    #[serde(default)]
    pub remove: Vec<u64>,
}

/// Puts every selected track in categories, or takes them out. Filling a
/// category from a label: select with `filters.labels_any`. Operators and
/// admins.
#[utoipa::path(post, path = "/tracks/categories", tag = "categories",
    request_body = CategoriesChange,
    responses((status = 200, body = SelectionResult), (status = "4XX", body = ErrorBody)))]
async fn apply_categories(
    user: CurrentUser,
    State(state): State<AppState>,
    Json(change): Json<CategoriesChange>,
) -> ApiResult<Json<SelectionResult>> {
    user.require(Role::Operator)?;
    if change.add.is_empty() && change.remove.is_empty() {
        return Err(ApiError::Invalid("no category to add or remove".to_owned()));
    }
    check_categories(&state.pool, &change.add, &change.remove).await?;
    let ids = change.selection.resolve(&state, MAX_SELECTION).await?;
    let mut tx = state.pool.begin().await?;
    let mut changed = 0;
    for chunk in ids.chunks(CHUNK) {
        for share in &change.add {
            let mut query =
                QueryBuilder::new("INSERT INTO track_categories (track_id, category_id, weight) ");
            query.push_values(chunk, |mut row, id| {
                row.push_bind(*id)
                    .push_bind(share.category_id)
                    .push_bind(share.weight);
            });
            query.push(" ON DUPLICATE KEY UPDATE weight = VALUES(weight)");
            changed += query.build().execute(&mut *tx).await?.rows_affected();
        }
        if !change.remove.is_empty() {
            let mut query =
                QueryBuilder::new("DELETE FROM track_categories WHERE category_id IN (");
            let mut separated = query.separated(", ");
            for category in &change.remove {
                separated.push_bind(*category);
            }
            query.push(") AND track_id IN (");
            let mut separated = query.separated(", ");
            for id in chunk {
                separated.push_bind(*id);
            }
            query.push(")");
            changed += query.build().execute(&mut *tx).await?.rows_affected();
        }
    }
    tx.commit().await?;
    tracing::info!(user = %user.username, tracks = ids.len(), changed, "categories changed");
    Ok(Json(SelectionResult {
        tracks: ids.len() as u64,
        changed,
    }))
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ProgrammingChange {
    pub selection: TrackSelection,
    /// A value of the mood scale, or `null` to clear it. Missing: kept.
    #[serde(default, deserialize_with = "some_or_null")]
    #[schema(value_type = Option<String>)]
    pub mood: Option<Option<String>>,
    /// A value of the sound code scale, or `null` to clear it.
    #[serde(default, deserialize_with = "some_or_null")]
    #[schema(value_type = Option<String>)]
    pub sound_code: Option<Option<String>>,
    /// The tempo, replacing the measured one; `null` goes back to it.
    #[serde(default, deserialize_with = "some_or_null")]
    #[schema(value_type = Option<f64>)]
    pub bpm: Option<Option<f64>>,
}

/// Sets the mood, sound code or tempo of every selected track. Operators
/// and admins.
#[utoipa::path(post, path = "/tracks/programming", tag = "library",
    request_body = ProgrammingChange,
    responses((status = 200, body = SelectionResult), (status = "4XX", body = ErrorBody)))]
async fn apply_programming(
    user: CurrentUser,
    State(state): State<AppState>,
    Json(change): Json<ProgrammingChange>,
) -> ApiResult<Json<SelectionResult>> {
    user.require(Role::Operator)?;
    check_scales(&state.pool, &change.mood, &change.sound_code).await?;
    let bpm = match change.bpm {
        Some(Some(bpm)) => {
            let bpm = TagField::Bpm
                .normalize(&bpm.to_string())
                .map_err(ApiError::Invalid)?;
            Some(Some(
                bpm.parse::<f64>()
                    .map_err(|error| ApiError::Internal(error.to_string()))?,
            ))
        }
        other => other,
    };
    let mut columns: Vec<(&str, Option<String>, Option<f64>)> = Vec::new();
    if let Some(mood) = &change.mood {
        columns.push(("mood", mood.clone(), None));
    }
    if let Some(sound_code) = &change.sound_code {
        columns.push(("sound_code", sound_code.clone(), None));
    }
    if let Some(bpm) = bpm {
        columns.push(("bpm", None, bpm));
    }
    if columns.is_empty() {
        return Err(ApiError::Invalid("nothing to set".to_owned()));
    }
    let ids = change.selection.resolve(&state, MAX_SELECTION).await?;
    let mut tx = state.pool.begin().await?;
    let mut changed = 0;
    for chunk in ids.chunks(CHUNK) {
        for (column, text, number) in &columns {
            let mut query = QueryBuilder::new(format!(
                "INSERT INTO track_programming (track_id, {column}) "
            ));
            query.push_values(chunk, |mut row, id| {
                row.push_bind(*id);
                if *column == "bpm" {
                    row.push_bind(*number);
                } else {
                    row.push_bind(text.clone());
                }
            });
            query.push(format!(
                " ON DUPLICATE KEY UPDATE {column} = VALUES({column})"
            ));
            let result = query.build().execute(&mut *tx).await?;
            // 1 a row added, 2 a row changed, 0 unchanged.
            changed += result.rows_affected().min(chunk.len() as u64);
        }
    }
    tx.commit().await?;
    tracing::info!(user = %user.username, tracks = ids.len(), "programming details set");
    Ok(Json(SelectionResult {
        tracks: ids.len() as u64,
        changed,
    }))
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct TagEdit {
    pub selection: TrackSelection,
    /// Fields set to the same value on every track; `null` removes the
    /// field. Fields left out keep each track's value.
    #[serde(default)]
    #[schema(value_type = std::collections::HashMap<String, Option<String>>)]
    pub set: FieldValues,
    /// Programming details written from each track's own values (ISRC,
    /// record label, BPM, mood); tracks without the value keep the file's.
    #[serde(default)]
    pub from_details: Vec<TagField>,
}

/// Writes tags into the files of the selected tracks (at most 10,000), as
/// one batch that can be undone: queues a library task for
/// heron-librarian. Files are not moved. Operators and admins.
#[utoipa::path(post, path = "/tracks/tags", tag = "library",
    request_body = TagEdit,
    responses((status = 202, body = LibraryTask), (status = "4XX", body = ErrorBody)))]
async fn edit_tags(
    user: CurrentUser,
    State(state): State<AppState>,
    Json(edit): Json<TagEdit>,
) -> ApiResult<(axum::http::StatusCode, Json<LibraryTask>)> {
    user.require(Role::Operator)?;
    let mut set = FieldValues::new();
    for (field, value) in &edit.set {
        let value = match value {
            None => None,
            Some(value) => Some(
                field
                    .normalize(value)
                    .map_err(|error| ApiError::Invalid(format!("{field:?}: {error}")))?,
            ),
        };
        set.insert(*field, value);
    }
    if let Some(field) = edit.from_details.iter().find(|field| !field.is_detail()) {
        return Err(ApiError::Invalid(format!(
            "{field:?} is not a programming detail"
        )));
    }
    if set.is_empty() && edit.from_details.is_empty() {
        return Err(ApiError::Invalid("no field to write".to_owned()));
    }
    if edit
        .from_details
        .iter()
        .any(|field| set.contains_key(field))
    {
        return Err(ApiError::Invalid(
            "a field is set and written from the details at once".to_owned(),
        ));
    }
    let track_ids = edit.selection.resolve(&state, MAX_TAG_EDIT).await?;
    if track_ids.is_empty() {
        return Err(ApiError::Invalid("no track selected".to_owned()));
    }
    let request = TaskRequest::EditTags {
        track_ids,
        set,
        from_details: edit.from_details,
    };
    let task = librarian::ask(&state, &user, request).await?;
    Ok((axum::http::StatusCode::ACCEPTED, Json(task)))
}
