// SPDX-License-Identifier: AGPL-3.0-or-later

//! Restrictions: hard rules on tracks, artists and categories (dayparting,
//! date ranges, sequences), never relaxed. See `docs/rotation.md`.

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use heron_rotation::{Rule, Target};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::app::AppState;
use crate::auth::{CurrentUser, Role};
use crate::error::{ApiError, ApiResult, ErrorBody};

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_restrictions, create_restriction))
        .routes(routes!(update_restriction, delete_restriction))
}

#[derive(Debug, Serialize, ToSchema)]
pub struct RestrictionView {
    pub id: u64,
    pub target: Target,
    /// The track (`artist – title`), artist or category, for people.
    pub target_name: Option<String>,
    pub rule: Rule,
    pub enabled: bool,
    pub note: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RestrictionInput {
    pub target: Target,
    pub rule: Rule,
    #[serde(default = "yes")]
    pub enabled: bool,
    pub note: Option<String>,
}

fn yes() -> bool {
    true
}

fn target_parts(target: Target) -> (&'static str, u64) {
    match target {
        Target::Track(id) => ("track", id),
        Target::Artist(id) => ("artist", id),
        Target::Category(id) => ("category", id),
    }
}

impl RestrictionInput {
    async fn check(&self, state: &AppState) -> ApiResult<Option<String>> {
        self.rule.check().map_err(ApiError::Invalid)?;
        let (kind, id) = target_parts(self.target);
        let table = match kind {
            "track" => "tracks",
            "artist" => "artists",
            _ => "categories",
        };
        let found: Option<u64> = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
            "SELECT id FROM {table} WHERE id = ?"
        )))
        .bind(id)
        .fetch_optional(&state.pool)
        .await?;
        if found.is_none() {
            return Err(ApiError::Invalid(format!("no {kind} {id}")));
        }
        if let Rule::NotAfter { category_id } = self.rule {
            let found: Option<u64> = sqlx::query_scalar("SELECT id FROM categories WHERE id = ?")
                .bind(category_id)
                .fetch_optional(&state.pool)
                .await?;
            if found.is_none() {
                return Err(ApiError::Invalid(format!("no category {category_id}")));
            }
        }
        let note = self
            .note
            .as_deref()
            .map(str::trim)
            .filter(|note| !note.is_empty());
        if note.is_some_and(|note| note.chars().count() > 512) {
            return Err(ApiError::Invalid(
                "notes are up to 512 characters".to_owned(),
            ));
        }
        Ok(note.map(str::to_owned))
    }

    fn rule_json(&self) -> ApiResult<String> {
        serde_json::to_string(&self.rule).map_err(|error| ApiError::Internal(error.to_string()))
    }
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct RestrictionQuery {
    /// `track`, `artist` or `category`.
    pub target: Option<String>,
    pub target_id: Option<u64>,
}

type Row = (
    u64,
    String,
    u64,
    Option<String>,
    String,
    bool,
    Option<String>,
);

const SELECT: &str = "SELECT r.id, CAST(r.target AS CHAR), r.target_id,
        CASE r.target
            WHEN 'track' THEN (SELECT CONCAT_WS(' – ', t.artist, t.title) FROM tracks t
                               WHERE t.id = r.target_id)
            WHEN 'artist' THEN (SELECT a.name FROM artists a WHERE a.id = r.target_id)
            ELSE (SELECT c.name FROM categories c WHERE c.id = r.target_id)
        END,
        CAST(r.rule AS CHAR), r.enabled, r.note
    FROM restrictions r";

fn view(
    (id, target, target_id, target_name, rule, enabled, note): Row,
) -> ApiResult<RestrictionView> {
    Ok(RestrictionView {
        id,
        target: crate::rotation::target(&target, target_id)
            .ok_or_else(|| ApiError::Internal(format!("restriction {id}: target {target}")))?,
        target_name,
        rule: serde_json::from_str(&rule)
            .map_err(|error| ApiError::Internal(format!("restriction {id}: {error}")))?,
        enabled,
        note,
    })
}

async fn find(state: &AppState, id: u64) -> ApiResult<RestrictionView> {
    let row: Option<Row> = sqlx::query_as(sqlx::AssertSqlSafe(format!("{SELECT} WHERE r.id = ?")))
        .bind(id)
        .fetch_optional(&state.pool)
        .await?;
    view(row.ok_or_else(|| ApiError::not_found("restriction"))?)
}

#[utoipa::path(get, path = "/restrictions", tag = "rotation",
    params(RestrictionQuery),
    responses((status = 200, body = Vec<RestrictionView>), (status = "4XX", body = ErrorBody)))]
async fn list_restrictions(
    _user: CurrentUser,
    State(state): State<AppState>,
    Query(query): Query<RestrictionQuery>,
) -> ApiResult<Json<Vec<RestrictionView>>> {
    let mut sql = sqlx::QueryBuilder::new(format!("{SELECT} WHERE TRUE"));
    if let Some(target) = &query.target {
        if !["track", "artist", "category"].contains(&target.as_str()) {
            return Err(ApiError::BadRequest(
                "target is track, artist or category".to_owned(),
            ));
        }
        sql.push(" AND r.target = ").push_bind(target.clone());
    }
    if let Some(id) = query.target_id {
        sql.push(" AND r.target_id = ").push_bind(id);
    }
    sql.push(" ORDER BY r.target, r.target_id, r.id");
    let rows: Vec<Row> = sql.build_query_as().fetch_all(&state.pool).await?;
    rows.into_iter()
        .map(view)
        .collect::<ApiResult<_>>()
        .map(Json)
}

/// Operators and admins.
#[utoipa::path(post, path = "/restrictions", tag = "rotation",
    request_body = RestrictionInput,
    responses((status = 201, body = RestrictionView), (status = "4XX", body = ErrorBody)))]
async fn create_restriction(
    user: CurrentUser,
    State(state): State<AppState>,
    Json(input): Json<RestrictionInput>,
) -> ApiResult<(StatusCode, Json<RestrictionView>)> {
    user.require(Role::Operator)?;
    let note = input.check(&state).await?;
    let (target, target_id) = target_parts(input.target);
    let id = sqlx::query(
        "INSERT INTO restrictions (target, target_id, rule, enabled, note) VALUES (?, ?, ?, ?, ?)",
    )
    .bind(target)
    .bind(target_id)
    .bind(input.rule_json()?)
    .bind(input.enabled)
    .bind(note)
    .execute(&state.pool)
    .await?
    .last_insert_id();
    tracing::info!(user = %user.username, restriction = id, "restriction added");
    state.queue.wake();
    Ok((StatusCode::CREATED, Json(find(&state, id).await?)))
}

/// Operators and admins.
#[utoipa::path(put, path = "/restrictions/{id}", tag = "rotation",
    params(("id" = u64, Path)), request_body = RestrictionInput,
    responses((status = 200, body = RestrictionView), (status = "4XX", body = ErrorBody)))]
async fn update_restriction(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<u64>,
    Json(input): Json<RestrictionInput>,
) -> ApiResult<Json<RestrictionView>> {
    user.require(Role::Operator)?;
    let note = input.check(&state).await?;
    let (target, target_id) = target_parts(input.target);
    let updated = sqlx::query(
        "UPDATE restrictions SET target = ?, target_id = ?, rule = ?, enabled = ?, note = ?
         WHERE id = ?",
    )
    .bind(target)
    .bind(target_id)
    .bind(input.rule_json()?)
    .bind(input.enabled)
    .bind(note)
    .bind(id)
    .execute(&state.pool)
    .await?;
    if updated.rows_affected() == 0 {
        // Unchanged, or missing.
        find(&state, id).await?;
    }
    state.queue.wake();
    Ok(Json(find(&state, id).await?))
}

/// Operators and admins.
#[utoipa::path(delete, path = "/restrictions/{id}", tag = "rotation",
    params(("id" = u64, Path)),
    responses((status = 204), (status = "4XX", body = ErrorBody)))]
async fn delete_restriction(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<u64>,
) -> ApiResult<StatusCode> {
    user.require(Role::Operator)?;
    let deleted = sqlx::query("DELETE FROM restrictions WHERE id = ?")
        .bind(id)
        .execute(&state.pool)
        .await?;
    if deleted.rows_affected() == 0 {
        return Err(ApiError::not_found("restriction"));
    }
    state.queue.wake();
    Ok(StatusCode::NO_CONTENT)
}
