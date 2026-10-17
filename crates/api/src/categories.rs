// SPDX-License-Identifier: AGPL-3.0-or-later

//! Categories: groups of tracks the clockwheels call (M6). A track can be
//! in several categories.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::app::AppState;
use crate::auth::{CurrentUser, Role};
use crate::error::{ApiError, ApiResult, ErrorBody, is_duplicate, is_still_used};

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list, create))
        .routes(routes!(get_one, update, delete))
}

#[derive(Clone, Debug, Serialize, ToSchema, sqlx::FromRow)]
pub struct Category {
    pub id: u64,
    pub name: String,
    /// `#rrggbb`.
    pub color: Option<String>,
    pub tracks: i64,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CategoryInput {
    pub name: String,
    /// `#rrggbb`, or none.
    pub color: Option<String>,
}

impl CategoryInput {
    fn check(&self) -> ApiResult<(String, Option<String>)> {
        let name = self.name.trim();
        if name.is_empty() || name.chars().count() > 255 {
            return Err(ApiError::Invalid(
                "names are 1 to 255 characters".to_owned(),
            ));
        }
        let color = match self.color.as_deref().map(str::trim) {
            None | Some("") => None,
            Some(color)
                if color.len() == 7
                    && color.starts_with('#')
                    && color[1..].chars().all(|c| c.is_ascii_hexdigit()) =>
            {
                Some(color.to_ascii_lowercase())
            }
            Some(_) => return Err(ApiError::Invalid("colors are written #rrggbb".to_owned())),
        };
        Ok((name.to_owned(), color))
    }
}

const SELECT: &str = "SELECT c.id, c.name, c.color,
           (SELECT COUNT(*) FROM track_categories tc WHERE tc.category_id = c.id) AS tracks
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
async fn list(_user: CurrentUser, State(state): State<AppState>) -> ApiResult<Json<Vec<Category>>> {
    let categories = sqlx::query_as(sqlx::AssertSqlSafe(format!("{SELECT} ORDER BY c.name")))
        .fetch_all(&state.pool)
        .await?;
    Ok(Json(categories))
}

#[utoipa::path(get, path = "/categories/{id}", tag = "categories",
    params(("id" = u64, Path)),
    responses((status = 200, body = Category), (status = "4XX", body = ErrorBody)))]
async fn get_one(
    _user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<u64>,
) -> ApiResult<Json<Category>> {
    sqlx::query_as(sqlx::AssertSqlSafe(format!("{SELECT} WHERE c.id = ?")))
        .bind(id)
        .fetch_optional(&state.pool)
        .await?
        .map(Json)
        .ok_or_else(|| ApiError::not_found("category"))
}

/// Operators and admins.
#[utoipa::path(post, path = "/categories", tag = "categories",
    request_body = CategoryInput,
    responses((status = 201, body = Category), (status = "4XX", body = ErrorBody)))]
async fn create(
    user: CurrentUser,
    State(state): State<AppState>,
    Json(input): Json<CategoryInput>,
) -> ApiResult<(StatusCode, Json<Category>)> {
    user.require(Role::Operator)?;
    let (name, color) = input.check()?;
    let id = sqlx::query("INSERT INTO categories (name, color) VALUES (?, ?)")
        .bind(name)
        .bind(color)
        .execute(&state.pool)
        .await
        .map_err(conflict)?
        .last_insert_id();
    let Json(category) = get_one(user, State(state), Path(id)).await?;
    Ok((StatusCode::CREATED, Json(category)))
}

/// Renames or recolors a category. Operators and admins.
#[utoipa::path(put, path = "/categories/{id}", tag = "categories",
    params(("id" = u64, Path)), request_body = CategoryInput,
    responses((status = 200, body = Category), (status = "4XX", body = ErrorBody)))]
async fn update(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<u64>,
    Json(input): Json<CategoryInput>,
) -> ApiResult<Json<Category>> {
    user.require(Role::Operator)?;
    let (name, color) = input.check()?;
    sqlx::query("UPDATE categories SET name = ?, color = ? WHERE id = ?")
        .bind(name)
        .bind(color)
        .bind(id)
        .execute(&state.pool)
        .await
        .map_err(conflict)?;
    get_one(user, State(state), Path(id)).await
}

/// Deletes a category; its tracks stay in the library. Refused while a
/// clock uses it. Operators and admins.
#[utoipa::path(delete, path = "/categories/{id}", tag = "categories",
    params(("id" = u64, Path)),
    responses((status = 204), (status = "4XX", body = ErrorBody)))]
async fn delete(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<u64>,
) -> ApiResult<StatusCode> {
    user.require(Role::Operator)?;
    let deleted = sqlx::query("DELETE FROM categories WHERE id = ?")
        .bind(id)
        .execute(&state.pool)
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
        };
        assert_eq!(
            input(" Gold ", Some("#FFAA00")).check().unwrap(),
            ("Gold".to_owned(), Some("#ffaa00".to_owned()))
        );
        assert!(input("", None).check().is_err());
        assert!(input("Gold", Some("orange")).check().is_err());
        assert_eq!(input("Gold", Some("")).check().unwrap().1, None);
    }
}
