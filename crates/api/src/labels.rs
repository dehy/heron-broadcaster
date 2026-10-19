// SPDX-License-Identifier: AGPL-3.0-or-later

//! Labels: keywords on tracks (`christmas`, `instrumental`, `live`), several
//! per track, to sort the library. They live in Heron only: files are not
//! modified. Labels are not categories (clocks call categories), but a
//! category can be filled from a label (`POST /tracks/categories` with a
//! label filter). Adding and removing labels on a selection is
//! `POST /tracks/labels`.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::app::AppState;
use crate::auth::{CurrentUser, Role};
use crate::categories::check_color;
use crate::error::{ApiError, ApiResult, ErrorBody, is_duplicate};

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_labels, create_label))
        .routes(routes!(update_label, delete_label))
        .routes(routes!(merge_label))
}

#[derive(Clone, Debug, Serialize, ToSchema, sqlx::FromRow)]
pub struct Label {
    pub id: u64,
    pub name: String,
    /// `#rrggbb`.
    pub color: Option<String>,
    pub tracks: i64,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct LabelInput {
    /// Unique, ignoring case.
    pub name: String,
    /// `#rrggbb`, or none.
    pub color: Option<String>,
}

impl LabelInput {
    fn check(&self) -> ApiResult<(String, Option<String>)> {
        let name = self.name.trim();
        if name.is_empty() || name.chars().count() > 100 {
            return Err(ApiError::Invalid(
                "label names are 1 to 100 characters".to_owned(),
            ));
        }
        Ok((name.to_owned(), check_color(self.color.as_deref())?))
    }
}

const SELECT: &str = "SELECT l.id, l.name, l.color,
           (SELECT COUNT(*) FROM track_labels tl JOIN tracks t ON t.id = tl.track_id
            WHERE tl.label_id = l.id AND t.deleted_at IS NULL) AS tracks
    FROM labels l";

fn conflict(error: sqlx::Error) -> ApiError {
    if is_duplicate(&error) {
        ApiError::Conflict("another label has this name: merge them instead".to_owned())
    } else {
        error.into()
    }
}

async fn find(state: &AppState, id: u64) -> ApiResult<Label> {
    sqlx::query_as(sqlx::AssertSqlSafe(format!("{SELECT} WHERE l.id = ?")))
        .bind(id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| ApiError::not_found("label"))
}

/// Every label, with how many tracks have it.
#[utoipa::path(get, path = "/labels", tag = "labels",
    responses((status = 200, body = Vec<Label>), (status = "4XX", body = ErrorBody)))]
async fn list_labels(
    _user: CurrentUser,
    State(state): State<AppState>,
) -> ApiResult<Json<Vec<Label>>> {
    let labels = sqlx::query_as(sqlx::AssertSqlSafe(format!("{SELECT} ORDER BY l.name")))
        .fetch_all(&state.pool)
        .await?;
    Ok(Json(labels))
}

/// Operators and admins.
#[utoipa::path(post, path = "/labels", tag = "labels",
    request_body = LabelInput,
    responses((status = 201, body = Label), (status = "4XX", body = ErrorBody)))]
async fn create_label(
    user: CurrentUser,
    State(state): State<AppState>,
    Json(input): Json<LabelInput>,
) -> ApiResult<(StatusCode, Json<Label>)> {
    user.require(Role::Operator)?;
    let (name, color) = input.check()?;
    let id = sqlx::query("INSERT INTO labels (name, color) VALUES (?, ?)")
        .bind(name)
        .bind(color)
        .execute(&state.pool)
        .await
        .map_err(conflict)?
        .last_insert_id();
    Ok((StatusCode::CREATED, Json(find(&state, id).await?)))
}

/// Renames or recolors a label. Operators and admins.
#[utoipa::path(put, path = "/labels/{id}", tag = "labels",
    params(("id" = u64, Path)), request_body = LabelInput,
    responses((status = 200, body = Label), (status = "4XX", body = ErrorBody)))]
async fn update_label(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<u64>,
    Json(input): Json<LabelInput>,
) -> ApiResult<Json<Label>> {
    user.require(Role::Operator)?;
    let (name, color) = input.check()?;
    sqlx::query("UPDATE labels SET name = ?, color = ? WHERE id = ?")
        .bind(name)
        .bind(color)
        .bind(id)
        .execute(&state.pool)
        .await
        .map_err(conflict)?;
    Ok(Json(find(&state, id).await?))
}

/// Deletes a label: its tracks lose it. Operators and admins.
#[utoipa::path(delete, path = "/labels/{id}", tag = "labels",
    params(("id" = u64, Path)),
    responses((status = 204), (status = "4XX", body = ErrorBody)))]
async fn delete_label(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<u64>,
) -> ApiResult<StatusCode> {
    user.require(Role::Operator)?;
    let deleted = sqlx::query("DELETE FROM labels WHERE id = ?")
        .bind(id)
        .execute(&state.pool)
        .await?;
    if deleted.rows_affected() == 0 {
        return Err(ApiError::not_found("label"));
    }
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct LabelMerge {
    /// The label that stays.
    pub into_id: u64,
}

/// Merges a label into another: its tracks get the other label, and it is
/// deleted. Operators and admins.
#[utoipa::path(post, path = "/labels/{id}/merge", tag = "labels",
    params(("id" = u64, Path)), request_body = LabelMerge,
    responses((status = 200, body = Label), (status = "4XX", body = ErrorBody)))]
async fn merge_label(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<u64>,
    Json(merge): Json<LabelMerge>,
) -> ApiResult<Json<Label>> {
    user.require(Role::Operator)?;
    if merge.into_id == id {
        return Err(ApiError::Invalid(
            "a label cannot be merged into itself".to_owned(),
        ));
    }
    let mut tx = state.pool.begin().await?;
    let found: Vec<u64> = sqlx::query_scalar("SELECT id FROM labels WHERE id IN (?, ?) FOR UPDATE")
        .bind(id)
        .bind(merge.into_id)
        .fetch_all(&mut *tx)
        .await?;
    if found.len() < 2 {
        return Err(ApiError::not_found("label"));
    }
    sqlx::query(
        "INSERT IGNORE INTO track_labels (track_id, label_id)
         SELECT track_id, ? FROM track_labels WHERE label_id = ?",
    )
    .bind(merge.into_id)
    .bind(id)
    .execute(&mut *tx)
    .await?;
    sqlx::query("DELETE FROM labels WHERE id = ?")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    tracing::info!(user = %user.username, from = id, into = merge.into_id, "labels merged");
    Ok(Json(find(&state, merge.into_id).await?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checks_names() {
        let input = |name: &str| LabelInput {
            name: name.to_owned(),
            color: Some("#AA0000".to_owned()),
        };
        assert_eq!(
            input(" christmas ").check().unwrap(),
            ("christmas".to_owned(), Some("#aa0000".to_owned()))
        );
        assert!(input(" ").check().is_err());
        assert!(input(&"x".repeat(101)).check().is_err());
    }
}
