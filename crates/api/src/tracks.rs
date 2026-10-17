// SPDX-License-Identifier: AGPL-3.0-or-later

//! The library as seen through the API: browsing and searching tracks,
//! their analysis, overrides and categories, and transition previews.
//!
//! The API never writes files: tags and moves belong to `heron-librarian`.

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use heron_core::playout::CrossfadeOverrides;
use heron_core::{PlayoutSettings, TrackOverrides, plan_transition};
use serde::{Deserialize, Serialize};
use sqlx::QueryBuilder;
use utoipa::{IntoParams, ToSchema};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::app::AppState;
use crate::auth::{CurrentUser, Role};
use crate::error::{ApiError, ApiResult, ErrorBody};
use crate::settings;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_tracks))
        .routes(routes!(get_track, patch_track))
        .routes(routes!(put_overrides, delete_overrides))
        .routes(routes!(put_categories))
        .routes(routes!(preview_transition))
}

#[derive(Clone, Copy, Debug, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum TrackFilter {
    /// Present, enabled, analyzed and audible: what autofill picks from.
    Playable,
    Missing,
    /// Not analyzed yet (or analysis failed).
    Unanalyzed,
    /// Analyzed, with nothing audible.
    Silent,
    Disabled,
    /// Identification not confident enough, waiting for review.
    NeedsReview,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum TrackSort {
    /// Artist, album, then path.
    #[default]
    Artist,
    Title,
    Album,
    /// Most recently added first.
    Added,
    /// Least recently played first, never played before all.
    LastPlayed,
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct TrackQuery {
    /// Words found in the title, artist, album or path.
    pub q: Option<String>,
    pub category_id: Option<u64>,
    #[param(inline)]
    pub filter: Option<TrackFilter>,
    #[param(inline)]
    pub sort: Option<TrackSort>,
    /// 1 to 500, 50 by default.
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

#[derive(Clone, Debug, Serialize, ToSchema, sqlx::FromRow)]
pub struct TrackSummary {
    pub id: u64,
    /// Relative to the library root.
    pub path: String,
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub duration_ms: Option<u32>,
    /// Disabled tracks are never picked by autofill or rotation.
    pub enabled: bool,
    pub missing: bool,
    pub analyzed: bool,
    pub silent: bool,
    pub last_played_at_unix_ms: Option<i64>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct TrackPage {
    pub items: Vec<TrackSummary>,
    /// Matching tracks in all pages.
    pub total: u64,
}

const SUMMARY_COLUMNS: &str = "
    t.id, t.path, t.title, t.artist, t.album, t.duration_ms, t.enabled,
    t.missing_since IS NOT NULL AS missing,
    a.track_id IS NOT NULL AS analyzed,
    COALESCE(a.cue_out_ms <= a.cue_in_ms, FALSE) AS silent,
    CAST(UNIX_TIMESTAMP(h.last_played) * 1000 AS SIGNED) AS last_played_at_unix_ms";

const SUMMARY_JOINS: &str = "
    FROM tracks t
    LEFT JOIN track_analysis a ON a.track_id = t.id
    LEFT JOIN (SELECT track_id, MAX(started_at) AS last_played FROM play_history
               WHERE track_id IS NOT NULL GROUP BY track_id) h ON h.track_id = t.id";

/// Escapes `%`, `_` and `\` for LIKE.
fn like_pattern(word: &str) -> String {
    let mut pattern = String::from("%");
    for c in word.chars() {
        if matches!(c, '%' | '_' | '\\') {
            pattern.push('\\');
        }
        pattern.push(c);
    }
    pattern.push('%');
    pattern
}

fn push_filters(query: &mut QueryBuilder<sqlx::MySql>, params: &TrackQuery) {
    query.push(" WHERE TRUE");
    for word in params.q.iter().flat_map(|q| q.split_whitespace()).take(10) {
        let pattern = like_pattern(word);
        // Paths compare as bytes: search case- and accent-insensitively.
        query.push(
            " AND CONVERT(CONCAT_WS(' ', t.title, t.artist, t.album, t.path) USING utf8mb4)
                  COLLATE utf8mb4_unicode_ci LIKE ",
        );
        query.push_bind(pattern);
    }
    if let Some(category_id) = params.category_id {
        query.push(" AND EXISTS (SELECT 1 FROM track_categories tc WHERE tc.track_id = t.id AND tc.category_id = ");
        query.push_bind(category_id);
        query.push(")");
    }
    let condition = match params.filter {
        None => "",
        Some(TrackFilter::Playable) => {
            " AND t.enabled AND t.missing_since IS NULL AND a.cue_out_ms > a.cue_in_ms"
        }
        Some(TrackFilter::Missing) => " AND t.missing_since IS NOT NULL",
        Some(TrackFilter::Unanalyzed) => " AND a.track_id IS NULL",
        Some(TrackFilter::Silent) => " AND a.cue_out_ms <= a.cue_in_ms",
        Some(TrackFilter::Disabled) => " AND NOT t.enabled",
        Some(TrackFilter::NeedsReview) => {
            " AND EXISTS (SELECT 1 FROM track_identifications i
                          WHERE i.track_id = t.id AND i.status = 'needs_review')"
        }
    };
    query.push(condition);
}

/// Library tracks, searched, filtered and sorted, a page at a time.
#[utoipa::path(get, path = "/tracks", tag = "library",
    params(TrackQuery),
    responses((status = 200, body = TrackPage), (status = "4XX", body = ErrorBody)))]
async fn list_tracks(
    _user: CurrentUser,
    State(state): State<AppState>,
    Query(params): Query<TrackQuery>,
) -> ApiResult<Json<TrackPage>> {
    let limit = params.limit.unwrap_or(50);
    if !(1..=500).contains(&limit) {
        return Err(ApiError::BadRequest("limit is 1 to 500".to_owned()));
    }
    let mut count = QueryBuilder::new(
        "SELECT CAST(COUNT(*) AS SIGNED) FROM tracks t
         LEFT JOIN track_analysis a ON a.track_id = t.id",
    );
    push_filters(&mut count, &params);
    let total: i64 = count.build_query_scalar().fetch_one(&state.pool).await?;

    let mut query = QueryBuilder::new(format!("SELECT {SUMMARY_COLUMNS} {SUMMARY_JOINS}"));
    push_filters(&mut query, &params);
    query.push(match params.sort.unwrap_or_default() {
        TrackSort::Artist => " ORDER BY t.artist IS NULL, t.artist, t.album, t.path",
        TrackSort::Title => " ORDER BY t.title IS NULL, t.title, t.artist",
        TrackSort::Album => " ORDER BY t.album IS NULL, t.album, t.path",
        TrackSort::Added => " ORDER BY t.created_at DESC, t.id DESC",
        TrackSort::LastPlayed => " ORDER BY h.last_played IS NOT NULL, h.last_played, t.id",
    });
    query.push(" LIMIT ").push_bind(limit);
    query.push(" OFFSET ").push_bind(params.offset.unwrap_or(0));
    let items = query.build_query_as().fetch_all(&state.pool).await?;
    Ok(Json(TrackPage {
        items,
        total: u64::try_from(total).unwrap_or(0),
    }))
}

#[derive(Clone, Debug, Serialize, ToSchema, sqlx::FromRow)]
pub struct TrackAnalysis {
    pub integrated_lufs: Option<f64>,
    pub true_peak_db: Option<f64>,
    pub cue_in_ms: u32,
    pub cue_out_ms: u32,
    /// The crossfade point: where the next track starts.
    pub fade_out_start_ms: Option<u32>,
    pub has_internal_silence: bool,
    pub analyzed_at_unix_ms: i64,
}

#[derive(Clone, Debug, Serialize, ToSchema, sqlx::FromRow)]
pub struct TrackCategory {
    pub category_id: u64,
    pub name: String,
    pub color: Option<String>,
    pub weight: f64,
}

#[derive(Clone, Debug, Serialize, ToSchema, sqlx::FromRow)]
pub struct TrackIdentification {
    /// `identified` or `needs_review`.
    pub status: String,
    /// Why the track needs review.
    pub reason: Option<String>,
    pub score: Option<f64>,
    pub recording_mbid: Option<String>,
    pub release_mbid: Option<String>,
}

/// How the engine will play the track: analysis and overrides combined.
#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct EffectivePlayout {
    pub cue_in_ms: u32,
    pub cue_out_ms: u32,
    pub mix_point_ms: Option<u32>,
    /// Loudness normalization gain with the current playout settings.
    pub gain_db: f64,
}

#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct TrackDetail {
    #[serde(flatten)]
    pub summary: TrackSummary,
    pub file_size: Option<u64>,
    pub added_at_unix_ms: i64,
    pub missing_since_unix_ms: Option<i64>,
    pub analysis: Option<TrackAnalysis>,
    pub overrides: TrackOverrides,
    pub playout: EffectivePlayout,
    pub categories: Vec<TrackCategory>,
    pub identification: Option<TrackIdentification>,
}

/// A track with its analysis, overrides, categories and identification.
#[utoipa::path(get, path = "/tracks/{id}", tag = "library",
    params(("id" = u64, Path)),
    responses((status = 200, body = TrackDetail), (status = "4XX", body = ErrorBody)))]
async fn get_track(
    _user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<u64>,
) -> ApiResult<Json<TrackDetail>> {
    let summary: TrackSummary = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT {SUMMARY_COLUMNS} {SUMMARY_JOINS} WHERE t.id = ?"
    )))
    .bind(id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| ApiError::not_found("track"))?;
    let (file_size, added_at_unix_ms, missing_since_unix_ms): (Option<u64>, i64, Option<i64>) =
        sqlx::query_as(
            "SELECT file_size, CAST(UNIX_TIMESTAMP(created_at) * 1000 AS SIGNED),
                    CAST(UNIX_TIMESTAMP(missing_since) * 1000 AS SIGNED)
             FROM tracks WHERE id = ?",
        )
        .bind(id)
        .fetch_one(&state.pool)
        .await?;
    let analysis: Option<TrackAnalysis> = sqlx::query_as(
        "SELECT integrated_lufs, true_peak_db, cue_in_ms, cue_out_ms, fade_out_start_ms,
                has_internal_silence,
                CAST(UNIX_TIMESTAMP(analyzed_at) * 1000 AS SIGNED) AS analyzed_at_unix_ms
         FROM track_analysis WHERE track_id = ?",
    )
    .bind(id)
    .fetch_optional(&state.pool)
    .await?;
    let categories: Vec<TrackCategory> = sqlx::query_as(
        "SELECT c.id AS category_id, c.name, c.color, tc.weight
         FROM track_categories tc JOIN categories c ON c.id = tc.category_id
         WHERE tc.track_id = ? ORDER BY c.name",
    )
    .bind(id)
    .fetch_all(&state.pool)
    .await?;
    let identification: Option<TrackIdentification> = sqlx::query_as(
        "SELECT CAST(status AS CHAR) AS status, reason, score, recording_mbid, release_mbid
         FROM track_identifications WHERE track_id = ?",
    )
    .bind(id)
    .fetch_optional(&state.pool)
    .await?;
    let overrides = load_overrides(&state, id).await?;
    let playout = state.queue.playout(id).await?;
    let settings: PlayoutSettings =
        settings::load(&state.pool, PlayoutSettings::SETTINGS_NAME).await?;
    Ok(Json(TrackDetail {
        summary,
        file_size,
        added_at_unix_ms,
        missing_since_unix_ms,
        analysis,
        overrides,
        playout: EffectivePlayout {
            cue_in_ms: playout.cue_in_ms,
            cue_out_ms: playout.cue_out_ms,
            mix_point_ms: playout.mix_point_ms,
            gain_db: playout.gain_db(&settings.loudness),
        },
        categories,
        identification,
    }))
}

async fn load_overrides(state: &AppState, id: u64) -> ApiResult<TrackOverrides> {
    type OverridesRow = (Option<u32>, Option<u32>, Option<bool>, Option<String>);
    let row: Option<OverridesRow> = sqlx::query_as(
        "SELECT cue_in_ms, cue_out_ms, gap_killer_enabled, CAST(crossfade AS CHAR)
         FROM track_overrides WHERE track_id = ?",
    )
    .bind(id)
    .fetch_optional(&state.pool)
    .await?;
    let Some((cue_in_ms, cue_out_ms, gap_killer_enabled, crossfade)) = row else {
        return Ok(TrackOverrides::default());
    };
    Ok(TrackOverrides {
        cue_in_ms,
        cue_out_ms,
        gap_killer_enabled,
        crossfade: crossfade
            .and_then(|json| serde_json::from_str(&json).ok())
            .unwrap_or_default(),
    })
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct TrackPatch {
    pub enabled: Option<bool>,
}

/// Enables or disables a track. Operators and admins.
#[utoipa::path(patch, path = "/tracks/{id}", tag = "library",
    params(("id" = u64, Path)), request_body = TrackPatch,
    responses((status = 200, body = TrackDetail), (status = "4XX", body = ErrorBody)))]
async fn patch_track(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<u64>,
    Json(patch): Json<TrackPatch>,
) -> ApiResult<Json<TrackDetail>> {
    user.require(Role::Operator)?;
    if let Some(enabled) = patch.enabled {
        sqlx::query("UPDATE tracks SET enabled = ? WHERE id = ?")
            .bind(enabled)
            .bind(id)
            .execute(&state.pool)
            .await?;
    }
    get_track(user, State(state), Path(id)).await
}

/// Replaces the track's overrides: cue points, Gap Killer, crossfade.
/// Missing fields use the analysis and station settings. Queued entries of
/// the track are updated within seconds. Operators and admins.
#[utoipa::path(put, path = "/tracks/{id}/overrides", tag = "library",
    params(("id" = u64, Path)), request_body = TrackOverrides,
    responses((status = 200, body = TrackOverrides), (status = "4XX", body = ErrorBody)))]
async fn put_overrides(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<u64>,
    Json(overrides): Json<TrackOverrides>,
) -> ApiResult<Json<TrackOverrides>> {
    user.require(Role::Operator)?;
    if let (Some(cue_in), Some(cue_out)) = (overrides.cue_in_ms, overrides.cue_out_ms)
        && cue_in >= cue_out
    {
        return Err(ApiError::Invalid(
            "cue_in_ms must be before cue_out_ms".to_owned(),
        ));
    }
    for fade in [overrides.crossfade.fade_in, overrides.crossfade.fade_out]
        .into_iter()
        .flatten()
    {
        if fade.level_pct > 100 {
            return Err(ApiError::Invalid("fade levels are 0 to 100 %".to_owned()));
        }
    }
    let crossfade = (overrides.crossfade != CrossfadeOverrides::default())
        .then(|| serde_json::to_string(&overrides.crossfade))
        .transpose()
        .map_err(|error| ApiError::Internal(error.to_string()))?;
    let result = sqlx::query(
        "INSERT INTO track_overrides (track_id, cue_in_ms, cue_out_ms, crossfade, gap_killer_enabled)
         VALUES (?, ?, ?, ?, ?)
         ON DUPLICATE KEY UPDATE cue_in_ms = VALUES(cue_in_ms), cue_out_ms = VALUES(cue_out_ms),
             crossfade = VALUES(crossfade), gap_killer_enabled = VALUES(gap_killer_enabled)",
    )
    .bind(id)
    .bind(overrides.cue_in_ms)
    .bind(overrides.cue_out_ms)
    .bind(crossfade)
    .bind(overrides.gap_killer_enabled)
    .execute(&state.pool)
    .await;
    match result {
        Err(error) if crate::error::is_still_used(&error) => {
            return Err(ApiError::not_found("track"));
        }
        other => other?,
    };
    state.queue.wake();
    Ok(Json(load_overrides(&state, id).await?))
}

/// Removes the track's overrides. Operators and admins.
#[utoipa::path(delete, path = "/tracks/{id}/overrides", tag = "library",
    params(("id" = u64, Path)),
    responses((status = 204), (status = "4XX", body = ErrorBody)))]
async fn delete_overrides(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<u64>,
) -> ApiResult<StatusCode> {
    user.require(Role::Operator)?;
    sqlx::query("DELETE FROM track_overrides WHERE track_id = ?")
        .bind(id)
        .execute(&state.pool)
        .await?;
    state.queue.wake();
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CategoryAssignment {
    pub category_id: u64,
    /// Relative chance of being picked within the category, 1 by default.
    #[serde(default = "one")]
    pub weight: f64,
}

fn one() -> f64 {
    1.0
}

/// Replaces the track's categories. Operators and admins.
#[utoipa::path(put, path = "/tracks/{id}/categories", tag = "library",
    params(("id" = u64, Path)), request_body = Vec<CategoryAssignment>,
    responses((status = 200, body = Vec<TrackCategory>), (status = "4XX", body = ErrorBody)))]
async fn put_categories(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<u64>,
    Json(categories): Json<Vec<CategoryAssignment>>,
) -> ApiResult<Json<Vec<TrackCategory>>> {
    user.require(Role::Operator)?;
    if categories
        .iter()
        .any(|c| !c.weight.is_finite() || c.weight <= 0.0)
    {
        return Err(ApiError::Invalid("weights must be above 0".to_owned()));
    }
    let mut tx = state.pool.begin().await?;
    let exists: Option<u64> = sqlx::query_scalar("SELECT id FROM tracks WHERE id = ? FOR UPDATE")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?;
    if exists.is_none() {
        return Err(ApiError::not_found("track"));
    }
    sqlx::query("DELETE FROM track_categories WHERE track_id = ?")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    for category in &categories {
        sqlx::query(
            "INSERT INTO track_categories (track_id, category_id, weight) VALUES (?, ?, ?)
             ON DUPLICATE KEY UPDATE weight = VALUES(weight)",
        )
        .bind(id)
        .bind(category.category_id)
        .bind(category.weight)
        .execute(&mut *tx)
        .await
        .map_err(|error| {
            if crate::error::is_still_used(&error) {
                ApiError::Invalid(format!("no category {}", category.category_id))
            } else {
                error.into()
            }
        })?;
    }
    tx.commit().await?;
    let assigned: Vec<TrackCategory> = sqlx::query_as(
        "SELECT c.id AS category_id, c.name, c.color, tc.weight
         FROM track_categories tc JOIN categories c ON c.id = tc.category_id
         WHERE tc.track_id = ? ORDER BY c.name",
    )
    .bind(id)
    .fetch_all(&state.pool)
    .await?;
    Ok(Json(assigned))
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct TransitionRequest {
    pub from_track_id: u64,
    pub to_track_id: u64,
    /// Settings to try; the stored ones when missing.
    pub settings: Option<PlayoutSettings>,
}

/// A gain ramp, in ms on its track's own timeline.
#[derive(Debug, Serialize, ToSchema)]
pub struct RampView {
    pub start_ms: u32,
    pub length_ms: u32,
    pub curve: heron_core::FadeCurve,
    /// Level at the quiet end of the ramp, 0 to 1.
    pub level: f32,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct TransitionPreview {
    /// When the incoming track starts, on the outgoing track's timeline.
    pub next_start_ms: u32,
    /// How long both tracks play at once (0: back to back).
    pub overlap_ms: u32,
    pub fade_out: Option<RampView>,
    pub fade_in: Option<RampView>,
    pub outgoing: EffectivePlayout,
    pub incoming: EffectivePlayout,
}

/// Plans the transition between two tracks the way the engine would, for
/// previews. Nothing is played.
#[utoipa::path(post, path = "/playout/transition", tag = "library",
    request_body = TransitionRequest,
    responses((status = 200, body = TransitionPreview), (status = "4XX", body = ErrorBody)))]
async fn preview_transition(
    _user: CurrentUser,
    State(state): State<AppState>,
    Json(request): Json<TransitionRequest>,
) -> ApiResult<Json<TransitionPreview>> {
    let settings = match request.settings {
        Some(settings) => settings,
        None => settings::load(&state.pool, PlayoutSettings::SETTINGS_NAME).await?,
    };
    let outgoing = state.queue.playout(request.from_track_id).await?;
    let incoming = state.queue.playout(request.to_track_id).await?;
    let transition = plan_transition(&outgoing, &incoming, &settings.crossfade);
    let ramp = |ramp: heron_core::playout::Ramp| RampView {
        start_ms: ramp.start_ms,
        length_ms: ramp.length_ms,
        curve: ramp.curve,
        level: ramp.level,
    };
    let effective = |playout: &heron_core::TrackPlayout| EffectivePlayout {
        cue_in_ms: playout.cue_in_ms,
        cue_out_ms: playout.cue_out_ms,
        mix_point_ms: playout.mix_point_ms,
        gain_db: playout.gain_db(&settings.loudness),
    };
    Ok(Json(TransitionPreview {
        next_start_ms: transition.next_start_ms,
        overlap_ms: transition.overlap_ms(&outgoing),
        fade_out: transition.fade_out.map(ramp),
        fade_in: transition.fade_in.map(ramp),
        outgoing: effective(&outgoing),
        incoming: effective(&incoming),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_words_are_escaped() {
        assert_eq!(like_pattern("50%_off\\"), "%50\\%\\_off\\\\%");
        assert_eq!(like_pattern("miles"), "%miles%");
    }
}
