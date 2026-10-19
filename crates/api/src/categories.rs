// SPDX-License-Identifier: AGPL-3.0-or-later

//! Categories: groups of tracks the clockwheels call. A track can be in
//! several categories. A category can override the separation windows of
//! the rotation settings (a power rotation repeats its titles sooner).

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use heron_rotation::SeparationOverrides;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::app::AppState;
use crate::auth::{CurrentUser, Role};
use crate::error::{ApiError, ApiResult, ErrorBody, is_duplicate, is_still_used};

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_categories, create_category))
        .routes(routes!(get_category, update_category, delete_category))
}

#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct Category {
    pub id: u64,
    pub name: String,
    /// `#rrggbb`.
    pub color: Option<String>,
    pub tracks: i64,
    /// Windows this category overrides; the others follow the rotation
    /// settings (clock slots can override them again).
    pub separation: SeparationOverrides,
}

type CategoryRow = (u64, String, Option<String>, i64, Option<String>);

fn category((id, name, color, tracks, separation): CategoryRow) -> Category {
    Category {
        id,
        name,
        color,
        tracks,
        separation: crate::rotation::overrides(separation.as_deref()),
    }
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CategoryInput {
    pub name: String,
    /// `#rrggbb`, or none.
    pub color: Option<String>,
    /// None overridden by default.
    #[serde(default)]
    pub separation: SeparationOverrides,
}

/// Overrides as stored: `NULL` for none.
pub fn separation_json(overrides: &SeparationOverrides) -> ApiResult<Option<String>> {
    let windows = heron_rotation::Windows::default().with(overrides);
    windows.check().map_err(ApiError::Invalid)?;
    if overrides.is_empty() {
        return Ok(None);
    }
    serde_json::to_string(overrides)
        .map(Some)
        .map_err(|error| ApiError::Internal(error.to_string()))
}

impl CategoryInput {
    fn check(&self) -> ApiResult<(String, Option<String>)> {
        let name = self.name.trim();
        if name.is_empty() || name.chars().count() > 255 {
            return Err(ApiError::Invalid(
                "names are 1 to 255 characters".to_owned(),
            ));
        }
        Ok((name.to_owned(), check_color(self.color.as_deref())?))
    }
}

/// A color as stored: `#rrggbb`, or none.
pub fn check_color(color: Option<&str>) -> ApiResult<Option<String>> {
    match color.map(str::trim) {
        None | Some("") => Ok(None),
        Some(color)
            if color.len() == 7
                && color.starts_with('#')
                && color[1..].chars().all(|c| c.is_ascii_hexdigit()) =>
        {
            Ok(Some(color.to_ascii_lowercase()))
        }
        Some(_) => Err(ApiError::Invalid("colors are written #rrggbb".to_owned())),
    }
}

const SELECT: &str = "SELECT c.id, c.name, c.color,
           (SELECT COUNT(*) FROM track_categories tc JOIN tracks t ON t.id = tc.track_id
            WHERE tc.category_id = c.id AND t.deleted_at IS NULL) AS tracks,
           CAST(c.separation AS CHAR)
    FROM categories c";

fn conflict(error: sqlx::Error) -> ApiError {
    if is_duplicate(&error) {
        ApiError::Conflict("another category has this name".to_owned())
    } else {
        error.into()
    }
}

#[utoipa::path(get, path = "/categories", tag = "categories",
    responses((status = 200, body = Vec<Category>), (status = "4XX", body = ErrorBody)))]
async fn list_categories(
    _user: CurrentUser,
    State(state): State<AppState>,
) -> ApiResult<Json<Vec<Category>>> {
    let rows: Vec<CategoryRow> =
        sqlx::query_as(sqlx::AssertSqlSafe(format!("{SELECT} ORDER BY c.name")))
            .fetch_all(&state.pool)
            .await?;
    Ok(Json(rows.into_iter().map(category).collect()))
}

#[utoipa::path(get, path = "/categories/{id}", tag = "categories",
    params(("id" = u64, Path)),
    responses((status = 200, body = Category), (status = "4XX", body = ErrorBody)))]
async fn get_category(
    _user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<u64>,
) -> ApiResult<Json<Category>> {
    let row: Option<CategoryRow> =
        sqlx::query_as(sqlx::AssertSqlSafe(format!("{SELECT} WHERE c.id = ?")))
            .bind(id)
            .fetch_optional(&state.pool)
            .await?;
    row.map(|row| Json(category(row)))
        .ok_or_else(|| ApiError::not_found("category"))
}

/// Operators and admins.
#[utoipa::path(post, path = "/categories", tag = "categories",
    request_body = CategoryInput,
    responses((status = 201, body = Category), (status = "4XX", body = ErrorBody)))]
async fn create_category(
    user: CurrentUser,
    State(state): State<AppState>,
    Json(input): Json<CategoryInput>,
) -> ApiResult<(StatusCode, Json<Category>)> {
    user.require(Role::Operator)?;
    let (name, color) = input.check()?;
    let separation = separation_json(&input.separation)?;
    let id = sqlx::query("INSERT INTO categories (name, color, separation) VALUES (?, ?, ?)")
        .bind(name)
        .bind(color)
        .bind(separation)
        .execute(&state.pool)
        .await
        .map_err(conflict)?
        .last_insert_id();
    let Json(category) = get_category(user, State(state), Path(id)).await?;
    Ok((StatusCode::CREATED, Json(category)))
}

/// Renames, recolors a category or changes its separation. Operators and
/// admins.
#[utoipa::path(put, path = "/categories/{id}", tag = "categories",
    params(("id" = u64, Path)), request_body = CategoryInput,
    responses((status = 200, body = Category), (status = "4XX", body = ErrorBody)))]
async fn update_category(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<u64>,
    Json(input): Json<CategoryInput>,
) -> ApiResult<Json<Category>> {
    user.require(Role::Operator)?;
    let (name, color) = input.check()?;
    let separation = separation_json(&input.separation)?;
    sqlx::query("UPDATE categories SET name = ?, color = ?, separation = ? WHERE id = ?")
        .bind(name)
        .bind(color)
        .bind(separation)
        .bind(id)
        .execute(&state.pool)
        .await
        .map_err(conflict)?;
    get_category(user, State(state), Path(id)).await
}

/// Deletes a category and its restrictions; its tracks stay in the
/// library. Refused while a clock uses it. Operators and admins.
#[utoipa::path(delete, path = "/categories/{id}", tag = "categories",
    params(("id" = u64, Path)),
    responses((status = 204), (status = "4XX", body = ErrorBody)))]
async fn delete_category(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<u64>,
) -> ApiResult<StatusCode> {
    user.require(Role::Operator)?;
    let mut tx = state.pool.begin().await?;
    sqlx::query("DELETE FROM restrictions WHERE target = 'category' AND target_id = ?")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    let deleted = sqlx::query("DELETE FROM categories WHERE id = ?")
        .bind(id)
        .execute(&mut *tx)
        .await
        .map_err(|error| {
            if is_still_used(&error) {
                ApiError::Conflict("clocks still use this category".to_owned())
            } else {
                error.into()
            }
        })?;
    if deleted.rows_affected() == 0 {
        return Err(ApiError::not_found("category"));
    }
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checks_names_and_colors() {
        let input = |name: &str, color: Option<&str>| CategoryInput {
            name: name.to_owned(),
            color: color.map(str::to_owned),
            separation: SeparationOverrides::default(),
        };
        assert_eq!(
            input(" Gold ", Some("#FFAA00")).check().unwrap(),
            ("Gold".to_owned(), Some("#ffaa00".to_owned()))
        );
        assert!(input("", None).check().is_err());
        assert!(input("Gold", Some("orange")).check().is_err());
        assert_eq!(input("Gold", Some("")).check().unwrap().1, None);
    }

    #[test]
    fn stores_only_overridden_windows() {
        assert_eq!(
            separation_json(&SeparationOverrides::default()).unwrap(),
            None
        );
        let overrides = SeparationOverrides {
            track_min: Some(90),
            ..SeparationOverrides::default()
        };
        let json = separation_json(&overrides).unwrap().unwrap();
        assert_eq!(crate::rotation::overrides(Some(&json)), overrides);
        let too_long = SeparationOverrides {
            title_min: Some(20_000),
            ..SeparationOverrides::default()
        };
        assert!(separation_json(&too_long).is_err());
    }
}
