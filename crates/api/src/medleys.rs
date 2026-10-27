// SPDX-License-Identifier: AGPL-3.0-or-later

//! Medleys: song hooks and liners picked from a recipe, then mixed into a
//! library file by heron-librarian (the API cannot write files). Picking is
//! [`heron_core::medley::plan`]; the rendered file becomes a track like any
//! other, linked back to its medley by [`run_linker`]. See `docs/medleys.md`.

use std::collections::HashSet;
use std::time::Duration;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use heron_core::library::TaskRequest;
use heron_core::medley::{self, Candidate, Part, PartKind, Recipe};
use serde::{Deserialize, Serialize};
use sqlx::{MySqlPool, QueryBuilder};
use utoipa::ToSchema;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::app::AppState;
use crate::auth::{CurrentUser, Role};
use crate::error::{ApiError, ApiResult, ErrorBody, is_duplicate, is_still_used};
use crate::librarian::{self, LibraryTask};

const LINK_INTERVAL: Duration = Duration::from_secs(10);
/// The longest part of a medley.
const MAX_PART_MS: u32 = 120_000;
/// The most parts in a medley: 30 hooks and a liner after each, and one before.
const MAX_PARTS: usize = 61;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_medleys, create_medley))
        .routes(routes!(get_medley, update_medley, delete_medley))
        .routes(routes!(plan_medley))
        .routes(routes!(pick_part))
        .routes(routes!(render_medley))
}

/// A part, with what its track is.
#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct PartView {
    pub kind: PartKind,
    pub track_id: u64,
    /// On the file's timeline.
    pub from_ms: u32,
    pub to_ms: u32,
    pub title: Option<String>,
    pub artist: Option<String>,
    pub path: String,
}

#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct Medley {
    pub id: u64,
    pub name: String,
    /// Where the rendered medley goes.
    pub category_id: Option<u64>,
    pub category_name: Option<String>,
    pub recipe: Recipe,
    /// Its current version, in order (empty until rendered once).
    pub parts: Vec<PartView>,
    /// The rendered file, relative to the library root.
    pub path: Option<String>,
    /// The rendered file's track, once the scanner found it.
    pub track_id: Option<u64>,
    /// The last render asked: its library task.
    pub task_id: Option<u64>,
    /// `pending`, `running`, `done` or `failed`.
    pub task_status: Option<String>,
    pub task_error: Option<String>,
    pub rendered_at_unix_ms: Option<i64>,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct MedleyInput {
    pub name: String,
    pub category_id: Option<u64>,
    #[serde(default)]
    pub recipe: Recipe,
}

impl MedleyInput {
    fn check(&self) -> ApiResult<String> {
        let name = self.name.trim();
        if name.is_empty() || name.chars().count() > 255 {
            return Err(ApiError::Invalid(
                "names are 1 to 255 characters".to_owned(),
            ));
        }
        self.recipe.check().map_err(ApiError::Invalid)?;
        Ok(name.to_owned())
    }
}

/// Parts picked from the recipe, not saved.
#[derive(Debug, Serialize, ToSchema)]
pub struct MedleyPlan {
    pub parts: Vec<PartView>,
    /// Songs that can give a hook: fewer than the recipe's hooks means a
    /// shorter medley.
    pub qualified: u64,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct PickRequest {
    pub kind: PartKind,
    /// Tracks not to pick (those already in the plan).
    #[serde(default)]
    pub exclude: Vec<u64>,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RenderRequest {
    pub parts: Vec<Part>,
}

type MedleyRow = (
    u64,
    String,
    Option<u64>,
    Option<String>,
    String,
    Option<String>,
    Option<u64>,
    Option<u64>,
    Option<String>,
    Option<String>,
    Option<i64>,
);

const SELECT: &str = "SELECT m.id, m.name, m.category_id, c.name, CAST(m.recipe AS CHAR), m.path,
        m.track_id, m.task_id, CAST(lt.status AS CHAR), lt.error,
        CAST(UNIX_TIMESTAMP(m.rendered_at) * 1000 AS SIGNED)
    FROM medleys m
    LEFT JOIN categories c ON c.id = m.category_id
    LEFT JOIN library_tasks lt ON lt.id = m.task_id";

async fn load(pool: &MySqlPool, id: Option<u64>) -> ApiResult<Vec<Medley>> {
    let rows: Vec<MedleyRow> = match id {
        Some(id) => {
            sqlx::query_as(sqlx::AssertSqlSafe(format!("{SELECT} WHERE m.id = ?")))
                .bind(id)
                .fetch_all(pool)
                .await?
        }
        None => {
            sqlx::query_as(sqlx::AssertSqlSafe(format!("{SELECT} ORDER BY m.name")))
                .fetch_all(pool)
                .await?
        }
    };
    let mut medleys = Vec::with_capacity(rows.len());
    for (
        id,
        name,
        category_id,
        category_name,
        recipe,
        path,
        track_id,
        task_id,
        status,
        error,
        rendered,
    ) in rows
    {
        let recipe = serde_json::from_str(&recipe)
            .map_err(|error| ApiError::Internal(format!("medley {id}: {error}")))?;
        let parts: Vec<Part> = saved_parts(pool, id).await?;
        medleys.push(Medley {
            id,
            name,
            category_id,
            category_name,
            recipe,
            parts: views(pool, &parts).await?,
            path,
            track_id,
            task_id,
            task_status: status,
            task_error: error,
            rendered_at_unix_ms: rendered,
        });
    }
    Ok(medleys)
}

async fn find(pool: &MySqlPool, id: u64) -> ApiResult<Medley> {
    load(pool, Some(id))
        .await?
        .pop()
        .ok_or_else(|| ApiError::not_found("medley"))
}

async fn saved_parts(pool: &MySqlPool, id: u64) -> ApiResult<Vec<Part>> {
    let rows: Vec<(String, u64, u32, u32)> = sqlx::query_as(
        "SELECT CAST(kind AS CHAR), track_id, from_ms, to_ms FROM medley_parts
         WHERE medley_id = ? ORDER BY position",
    )
    .bind(id)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(kind, track_id, from_ms, to_ms)| Part {
            kind: if kind == "liner" {
                PartKind::Liner
            } else {
                PartKind::Hook
            },
            track_id,
            from_ms,
            to_ms,
        })
        .collect())
}

/// The parts with their tracks' title, artist and path.
async fn views(pool: &MySqlPool, parts: &[Part]) -> ApiResult<Vec<PartView>> {
    if parts.is_empty() {
        return Ok(Vec::new());
    }
    let mut query = QueryBuilder::new("SELECT id, title, artist, path FROM tracks WHERE id IN (");
    let mut separated = query.separated(", ");
    for part in parts {
        separated.push_bind(part.track_id);
    }
    query.push(")");
    let tracks: Vec<(u64, Option<String>, Option<String>, String)> =
        query.build_query_as().fetch_all(pool).await?;
    Ok(parts
        .iter()
        .filter_map(|part| {
            let (_, title, artist, path) = tracks.iter().find(|track| track.0 == part.track_id)?;
            Some(PartView {
                kind: part.kind,
                track_id: part.track_id,
                from_ms: part.from_ms,
                to_ms: part.to_ms,
                title: title.clone(),
                artist: artist.clone(),
                path: path.clone(),
            })
        })
        .collect())
}

/// The songs that can give a hook to this recipe.
async fn songs(pool: &MySqlPool, recipe: &Recipe) -> ApiResult<Vec<Candidate>> {
    let mut query = QueryBuilder::new(
        "SELECT t.id, m.hook_in_ms, m.hook_out_ms,
                CAST(COALESCE(mb.first_release_year, t.year) AS UNSIGNED),
                CAST(COALESCE(p.bpm, a.bpm) AS DOUBLE)
         FROM tracks t
         JOIN track_markers m ON m.track_id = t.id
         JOIN track_analysis a ON a.track_id = t.id
         LEFT JOIN track_musicbrainz mb ON mb.track_id = t.id
         LEFT JOIN track_programming p ON p.track_id = t.id
         WHERE t.deleted_at IS NULL AND t.enabled AND t.missing_since IS NULL
           AND a.cue_out_ms > a.cue_in_ms
           AND m.hook_in_ms IS NOT NULL AND m.hook_out_ms > m.hook_in_ms
           AND NOT EXISTS (SELECT 1 FROM medleys md WHERE md.track_id = t.id)
           AND EXISTS (SELECT 1 FROM track_categories tc
                       WHERE tc.track_id = t.id AND tc.category_id IN (",
    );
    let mut separated = query.separated(", ");
    for id in &recipe.category_ids {
        separated.push_bind(*id);
    }
    query.push("))");
    if let Some(label_id) = recipe.label_id {
        query
            .push(" AND EXISTS (SELECT 1 FROM track_labels tl WHERE tl.track_id = t.id AND tl.label_id = ")
            .push_bind(label_id)
            .push(")");
    }
    // Track, hook in and out, year, BPM.
    type Row = (u64, u32, u32, Option<u64>, Option<f64>);
    let rows: Vec<Row> = query.build_query_as().fetch_all(pool).await?;
    Ok(rows
        .into_iter()
        .map(|(track_id, from_ms, to_ms, year, bpm)| Candidate {
            track_id,
            from_ms,
            to_ms,
            year: year.and_then(|year| u16::try_from(year).ok()),
            bpm,
        })
        .collect())
}

/// The liners of the recipe's liner category, from cue-in to cue-out.
async fn liners(pool: &MySqlPool, recipe: &Recipe) -> ApiResult<Vec<Candidate>> {
    let Some(category_id) = recipe.liner_category_id else {
        return Ok(Vec::new());
    };
    let rows: Vec<(u64, u32, u32)> = sqlx::query_as(
        "SELECT t.id, COALESCE(o.cue_in_ms, a.cue_in_ms), COALESCE(o.cue_out_ms, a.cue_out_ms)
         FROM tracks t
         JOIN track_categories tc ON tc.track_id = t.id AND tc.category_id = ?
         JOIN track_analysis a ON a.track_id = t.id
         LEFT JOIN track_overrides o ON o.track_id = t.id
         WHERE t.deleted_at IS NULL AND t.enabled AND t.missing_since IS NULL
           AND a.cue_out_ms > a.cue_in_ms",
    )
    .bind(category_id)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .filter(|(_, from_ms, to_ms)| to_ms > from_ms)
        .map(|(track_id, from_ms, to_ms)| Candidate {
            track_id,
            from_ms,
            to_ms: to_ms.min(from_ms.saturating_add(MAX_PART_MS)),
            year: None,
            bpm: None,
        })
        .collect())
}

/// The tracks of the medley's current version: new picks avoid them.
async fn current_tracks(pool: &MySqlPool, id: u64) -> ApiResult<HashSet<u64>> {
    Ok(saved_parts(pool, id)
        .await?
        .into_iter()
        .map(|part| part.track_id)
        .collect())
}

/// Every medley, by name.
#[utoipa::path(get, path = "/medleys", tag = "medleys",
    responses((status = 200, body = Vec<Medley>), (status = "4XX", body = ErrorBody)))]
async fn list_medleys(
    _user: CurrentUser,
    State(state): State<AppState>,
) -> ApiResult<Json<Vec<Medley>>> {
    Ok(Json(load(&state.pool, None).await?))
}

#[utoipa::path(get, path = "/medleys/{id}", tag = "medleys", params(("id" = u64, Path)),
    responses((status = 200, body = Medley), (status = "4XX", body = ErrorBody)))]
async fn get_medley(
    _user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<u64>,
) -> ApiResult<Json<Medley>> {
    Ok(Json(find(&state.pool, id).await?))
}

fn conflict(error: sqlx::Error) -> ApiError {
    if is_duplicate(&error) {
        ApiError::Conflict("another medley has this name".to_owned())
    } else if is_still_used(&error) {
        ApiError::Invalid("no such category".to_owned())
    } else {
        error.into()
    }
}

fn recipe_json(recipe: &Recipe) -> ApiResult<String> {
    serde_json::to_string(recipe).map_err(|error| ApiError::Internal(error.to_string()))
}

/// Operators and admins.
#[utoipa::path(post, path = "/medleys", tag = "medleys", request_body = MedleyInput,
    responses((status = 201, body = Medley), (status = "4XX", body = ErrorBody)))]
async fn create_medley(
    user: CurrentUser,
    State(state): State<AppState>,
    Json(input): Json<MedleyInput>,
) -> ApiResult<(StatusCode, Json<Medley>)> {
    user.require(Role::Operator)?;
    let name = input.check()?;
    let id = sqlx::query("INSERT INTO medleys (name, category_id, recipe) VALUES (?, ?, ?)")
        .bind(name)
        .bind(input.category_id)
        .bind(recipe_json(&input.recipe)?)
        .execute(&state.pool)
        .await
        .map_err(conflict)?
        .last_insert_id();
    Ok((StatusCode::CREATED, Json(find(&state.pool, id).await?)))
}

/// Renames a medley or changes its recipe (applied on its next render).
/// Operators and admins.
#[utoipa::path(put, path = "/medleys/{id}", tag = "medleys", params(("id" = u64, Path)),
    request_body = MedleyInput,
    responses((status = 200, body = Medley), (status = "4XX", body = ErrorBody)))]
async fn update_medley(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<u64>,
    Json(input): Json<MedleyInput>,
) -> ApiResult<Json<Medley>> {
    user.require(Role::Operator)?;
    let name = input.check()?;
    let previous = find(&state.pool, id).await?;
    sqlx::query("UPDATE medleys SET name = ?, category_id = ?, recipe = ? WHERE id = ?")
        .bind(&name)
        .bind(input.category_id)
        .bind(recipe_json(&input.recipe)?)
        .bind(id)
        .execute(&state.pool)
        .await
        .map_err(conflict)?;
    if let Some(track_id) = previous.track_id {
        if previous.name != name {
            name_track(&state.pool, track_id, &name).await?;
        }
        if let Some(category_id) = input.category_id
            && previous.category_id != Some(category_id)
        {
            put_in_category(&state.pool, track_id, category_id).await?;
        }
    }
    Ok(Json(find(&state.pool, id).await?))
}

/// Deletes the recipe; the rendered track stays in the library. Operators
/// and admins.
#[utoipa::path(delete, path = "/medleys/{id}", tag = "medleys", params(("id" = u64, Path)),
    responses((status = 204), (status = "4XX", body = ErrorBody)))]
async fn delete_medley(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<u64>,
) -> ApiResult<StatusCode> {
    user.require(Role::Operator)?;
    let deleted = sqlx::query("DELETE FROM medleys WHERE id = ?")
        .bind(id)
        .execute(&state.pool)
        .await?
        .rows_affected();
    if deleted == 0 {
        return Err(ApiError::not_found("medley"));
    }
    Ok(StatusCode::NO_CONTENT)
}

/// Picks the parts from the recipe, without saving them. Operators and admins.
#[utoipa::path(post, path = "/medleys/{id}/plan", tag = "medleys", params(("id" = u64, Path)),
    responses((status = 200, body = MedleyPlan), (status = "4XX", body = ErrorBody)))]
async fn plan_medley(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<u64>,
) -> ApiResult<Json<MedleyPlan>> {
    user.require(Role::Operator)?;
    let medley = find(&state.pool, id).await?;
    let recipe = &medley.recipe;
    let songs = songs(&state.pool, recipe).await?;
    let liners = liners(&state.pool, recipe).await?;
    let avoid = current_tracks(&state.pool, id).await?;
    let parts = medley::plan(recipe, &songs, &liners, &avoid, crate::rotation::seed());
    Ok(Json(MedleyPlan {
        parts: views(&state.pool, &parts).await?,
        qualified: medley::qualified(recipe, &songs) as u64,
    }))
}

/// Another hook or liner, none of `exclude`, to replace one in a plan.
/// Operators and admins.
#[utoipa::path(post, path = "/medleys/{id}/pick", tag = "medleys", params(("id" = u64, Path)),
    request_body = PickRequest,
    responses((status = 200, body = PartView), (status = "4XX", body = ErrorBody)))]
async fn pick_part(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<u64>,
    Json(request): Json<PickRequest>,
) -> ApiResult<Json<PartView>> {
    user.require(Role::Operator)?;
    let medley = find(&state.pool, id).await?;
    let candidates = match request.kind {
        PartKind::Hook => songs(&state.pool, &medley.recipe).await?,
        PartKind::Liner => liners(&state.pool, &medley.recipe).await?,
    };
    let exclude: HashSet<u64> = request.exclude.into_iter().collect();
    let avoid = current_tracks(&state.pool, id).await?;
    let part = medley::pick_one(
        &medley.recipe,
        request.kind,
        &candidates,
        &exclude,
        &avoid,
        crate::rotation::seed(),
    )
    .ok_or_else(|| ApiError::Conflict("no other track to pick".to_owned()))?;
    views(&state.pool, &[part])
        .await?
        .pop()
        .map(Json)
        .ok_or_else(|| ApiError::not_found("track"))
}

/// Saves the parts as the medley's version and asks heron-librarian to
/// mix them into its file. Operators and admins.
#[utoipa::path(post, path = "/medleys/{id}/render", tag = "medleys", params(("id" = u64, Path)),
    request_body = RenderRequest,
    responses((status = 202, body = LibraryTask), (status = "4XX", body = ErrorBody)))]
async fn render_medley(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<u64>,
    Json(request): Json<RenderRequest>,
) -> ApiResult<(StatusCode, Json<LibraryTask>)> {
    user.require(Role::Operator)?;
    find(&state.pool, id).await?;
    let parts = request.parts;
    if parts.is_empty() || parts.len() > MAX_PARTS {
        return Err(ApiError::Invalid(format!(
            "a medley has 1 to {MAX_PARTS} parts"
        )));
    }
    if !parts.iter().any(|part| part.kind == PartKind::Hook) {
        return Err(ApiError::Invalid(
            "a medley has at least one hook".to_owned(),
        ));
    }
    if parts
        .iter()
        .any(|part| part.from_ms >= part.to_ms || part.length_ms() > MAX_PART_MS)
    {
        return Err(ApiError::Invalid(format!(
            "each part starts before it ends and lasts at most {} s",
            MAX_PART_MS / 1_000
        )));
    }
    let known = views(&state.pool, &parts).await?.len();
    if known != parts.len() {
        return Err(ApiError::Invalid(
            "a part's track does not exist".to_owned(),
        ));
    }
    let mut tx = state.pool.begin().await?;
    sqlx::query("DELETE FROM medley_parts WHERE medley_id = ?")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    let mut query = QueryBuilder::new(
        "INSERT INTO medley_parts (medley_id, position, kind, track_id, from_ms, to_ms) ",
    );
    query.push_values(parts.iter().enumerate(), |mut row, (position, part)| {
        row.push_bind(id)
            .push_bind(position as u32)
            .push_bind(match part.kind {
                PartKind::Hook => "hook",
                PartKind::Liner => "liner",
            })
            .push_bind(part.track_id)
            .push_bind(part.from_ms)
            .push_bind(part.to_ms);
    });
    query.build().execute(&mut *tx).await?;
    tx.commit().await?;
    let task = librarian::ask(&state, &user, TaskRequest::RenderMedley { medley_id: id }).await?;
    sqlx::query("UPDATE medleys SET task_id = ? WHERE id = ?")
        .bind(task.id)
        .bind(id)
        .execute(&state.pool)
        .await?;
    Ok((StatusCode::ACCEPTED, Json(task)))
}

/// Shows the medley's name as its track's title, kept in Heron's database
/// (a WAV file has no tags to read it from).
async fn name_track(pool: &MySqlPool, track_id: u64, name: &str) -> ApiResult<()> {
    sqlx::query(
        "INSERT INTO track_tag_overrides (track_id, field, value) VALUES (?, 'title', ?)
         ON DUPLICATE KEY UPDATE value = VALUES(value)",
    )
    .bind(track_id)
    .bind(name)
    .execute(pool)
    .await?;
    sqlx::query("UPDATE tracks SET title = ? WHERE id = ?")
        .bind(name)
        .bind(track_id)
        .execute(pool)
        .await?;
    Ok(())
}

async fn put_in_category(pool: &MySqlPool, track_id: u64, category_id: u64) -> ApiResult<()> {
    sqlx::query("INSERT IGNORE INTO track_categories (track_id, category_id) VALUES (?, ?)")
        .bind(track_id)
        .bind(category_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Links the medleys rendered since to their tracks, once the scanner found
/// their files: the track gets the medley's name and category.
pub async fn link(pool: &MySqlPool) -> ApiResult<usize> {
    let rows: Vec<(u64, String, Option<u64>, u64)> = sqlx::query_as(
        "SELECT m.id, m.name, m.category_id, t.id FROM medleys m
         JOIN tracks t ON t.path = m.path AND t.deleted_at IS NULL
         WHERE m.track_id IS NULL OR m.track_id <> t.id",
    )
    .fetch_all(pool)
    .await?;
    for (medley_id, name, category_id, track_id) in &rows {
        sqlx::query("UPDATE medleys SET track_id = ? WHERE id = ?")
            .bind(track_id)
            .bind(medley_id)
            .execute(pool)
            .await?;
        name_track(pool, *track_id, name).await?;
        if let Some(category_id) = category_id {
            put_in_category(pool, *track_id, *category_id).await?;
        }
    }
    Ok(rows.len())
}

/// Links rendered medleys to their tracks every few seconds.
pub async fn run_linker(pool: MySqlPool) {
    loop {
        match link(&pool).await {
            Ok(0) => {}
            Ok(linked) => tracing::info!(medleys = linked, "medleys linked to their tracks"),
            Err(error) => tracing::warn!(%error, "cannot link medleys to their tracks"),
        }
        tokio::time::sleep(LINK_INTERVAL).await;
    }
}
