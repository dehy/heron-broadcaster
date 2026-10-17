// SPDX-License-Identifier: AGPL-3.0-or-later

//! Play history, recorded from the engine's track changes (`play_history`).
//!
//! A row is added when a track starts and completed when it ends. While the
//! API is away the engine plays on and its events are lost: on
//! reconnection, the tracks still playing are matched with their open rows
//! (or recorded, started when their position says), and the other open
//! rows are closed without an end reason.

use std::collections::HashMap;
use std::time::SystemTime;

use axum::Json;
use axum::extract::{Query, State};
use heron_core::PlaySource;
use serde::{Deserialize, Serialize};
use sqlx::MySqlPool;
use tokio::sync::broadcast;
use utoipa::{IntoParams, ToSchema};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::app::AppState;
use crate::auth::CurrentUser;
use crate::engine::{DeckName, EndReason, LinkEvent, PlayerState, QueueEntry, TrackEvent, unix_ms};
use crate::error::{ApiError, ApiResult, ErrorBody};

/// A recorded play may be matched with what is on air if their start
/// times are this close.
const MATCH_TOLERANCE_MS: u64 = 5_000;

pub struct HistoryRecorder {
    pool: MySqlPool,
    /// Engine queue entry → `play_history` row, for the engine started at
    /// `engine_started_at`.
    open: HashMap<u64, u64>,
    engine_started_at: u64,
}

fn seconds(unix_ms: u64) -> f64 {
    unix_ms as f64 / 1_000.0
}

fn source_text(source: PlaySource) -> &'static str {
    match source {
        PlaySource::Rotation => "rotation",
        PlaySource::Request => "request",
        PlaySource::Fallback => "fallback",
        PlaySource::Manual => "manual",
    }
}

impl HistoryRecorder {
    pub fn new(pool: MySqlPool) -> Self {
        Self {
            pool,
            open: HashMap::new(),
            engine_started_at: 0,
        }
    }

    pub async fn run(mut self, mut events: broadcast::Receiver<LinkEvent>) {
        loop {
            let event = match events.recv().await {
                Ok(event) => event,
                Err(broadcast::error::RecvError::Lagged(missed)) => {
                    tracing::warn!(missed, "play history fell behind: plays were not recorded");
                    continue;
                }
                Err(broadcast::error::RecvError::Closed) => return,
            };
            let result = match event {
                LinkEvent::Connected(state) => self.reconcile(&state).await,
                LinkEvent::Track(event) => self.record(event).await,
            };
            if let Err(error) = result {
                tracing::error!(%error, "cannot record the play history");
            }
        }
    }

    async fn record(&mut self, event: TrackEvent) -> ApiResult<()> {
        match event {
            TrackEvent::Started {
                deck,
                entry,
                at_unix_ms,
            } => {
                if !self.open.contains_key(&entry.entry_id) {
                    let id = self.insert(deck, &entry, at_unix_ms).await?;
                    self.open.insert(entry.entry_id, id);
                }
            }
            TrackEvent::Ended {
                entry,
                at_unix_ms,
                played_ms,
                reason,
                ..
            } => {
                if let Some(id) = self.open.remove(&entry.entry_id) {
                    sqlx::query(
                        "UPDATE play_history SET ended_at = FROM_UNIXTIME(?), played_ms = ?,
                                end_reason = ?
                         WHERE id = ?",
                    )
                    .bind(seconds(at_unix_ms))
                    .bind(u32::try_from(played_ms).unwrap_or(u32::MAX))
                    .bind(reason.as_str())
                    .bind(id)
                    .execute(&self.pool)
                    .await?;
                }
            }
        }
        Ok(())
    }

    async fn insert(&self, deck: DeckName, entry: &QueueEntry, started_at: u64) -> ApiResult<u64> {
        let track = &entry.track;
        // The track may have been deleted since it was queued.
        let track_id = match track.track_id {
            Some(id) => {
                sqlx::query_scalar::<_, u64>("SELECT id FROM tracks WHERE id = ?")
                    .bind(id)
                    .fetch_optional(&self.pool)
                    .await?
            }
            None => None,
        };
        let id = sqlx::query(
            "INSERT INTO play_history (track_id, artist, title, started_at, deck, source)
             VALUES (?, ?, ?, FROM_UNIXTIME(?), ?, ?)",
        )
        .bind(track_id)
        .bind(&track.artist)
        .bind(&track.title)
        .bind(seconds(started_at))
        .bind(deck.as_str())
        .bind(source_text(track.source))
        .execute(&self.pool)
        .await?
        .last_insert_id();
        Ok(id)
    }

    /// Matches what plays now with the open rows, after a reconnection.
    async fn reconcile(&mut self, state: &PlayerState) -> ApiResult<()> {
        if state.engine_started_at_unix_ms != self.engine_started_at {
            self.engine_started_at = state.engine_started_at_unix_ms;
            self.open.clear();
        }
        let now = unix_ms(SystemTime::now());
        let mut keep = Vec::new();
        for deck in &state.decks {
            let Some(entry) = &deck.entry else {
                continue;
            };
            if let Some(id) = self.open.get(&entry.entry_id) {
                keep.push(*id);
                continue;
            }
            let started_at = now.saturating_sub(deck.position_ms);
            let open: Option<u64> = sqlx::query_scalar(
                "SELECT id FROM play_history
                 WHERE ended_at IS NULL AND deck = ? AND track_id <=> ?
                   AND ABS(TIMESTAMPDIFF(MICROSECOND, started_at, FROM_UNIXTIME(?))) < ?
                 ORDER BY started_at DESC LIMIT 1",
            )
            .bind(deck.deck.as_str())
            .bind(entry.track.track_id)
            .bind(seconds(started_at))
            .bind(MATCH_TOLERANCE_MS * 1_000)
            .fetch_optional(&self.pool)
            .await?;
            let id = match open {
                Some(id) => id,
                None => self.insert(deck.deck, entry, started_at).await?,
            };
            self.open.insert(entry.entry_id, id);
            keep.push(id);
        }
        let open_rows: Vec<u64> =
            sqlx::query_scalar("SELECT id FROM play_history WHERE ended_at IS NULL")
                .fetch_all(&self.pool)
                .await?;
        let mut closed = 0;
        for id in open_rows.into_iter().filter(|id| !keep.contains(id)) {
            // Ended while the API was away: when exactly is unknown.
            sqlx::query("UPDATE play_history SET ended_at = NOW(6) WHERE id = ?")
                .bind(id)
                .execute(&self.pool)
                .await?;
            closed += 1;
        }
        self.open.retain(|_, id| keep.contains(id));
        if closed > 0 {
            tracing::info!(closed, "closed plays that ended while the API was away");
        }
        Ok(())
    }
}

/// One play.
#[derive(Clone, Debug, Serialize, ToSchema, sqlx::FromRow)]
pub struct Play {
    pub id: u64,
    /// Missing for files outside the library, and tracks deleted since.
    pub track_id: Option<u64>,
    pub artist: Option<String>,
    pub title: Option<String>,
    pub album: Option<String>,
    pub started_at_unix_ms: i64,
    /// Missing while the track plays.
    pub ended_at_unix_ms: Option<i64>,
    pub played_ms: Option<u32>,
    /// `finished`, `skipped`, `stopped` or `replaced`; missing while the
    /// track plays, or when the API missed the end.
    #[schema(value_type = Option<EndReason>)]
    pub end_reason: Option<String>,
    #[schema(value_type = DeckName)]
    pub deck: String,
    #[schema(value_type = PlaySource)]
    pub source: String,
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct HistoryQuery {
    /// Plays started before this time (Unix ms), for the next page.
    pub before: Option<i64>,
    /// Plays started at or after this time (Unix ms).
    pub after: Option<i64>,
    /// Only this track.
    pub track_id: Option<u64>,
    /// Leave out the FX player (jingles played over the music).
    #[serde(default)]
    pub exclude_fx: bool,
    /// 1 to 500, 50 by default.
    pub limit: Option<u32>,
}

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(list))
}

/// Plays, most recent first.
#[utoipa::path(get, path = "/history", tag = "history",
    params(HistoryQuery),
    responses((status = 200, body = Vec<Play>), (status = "4XX", body = ErrorBody)))]
async fn list(
    _user: CurrentUser,
    State(state): State<AppState>,
    Query(query): Query<HistoryQuery>,
) -> ApiResult<Json<Vec<Play>>> {
    let limit = query.limit.unwrap_or(50);
    if !(1..=500).contains(&limit) {
        return Err(ApiError::BadRequest("limit is 1 to 500".to_owned()));
    }
    let mut sql = sqlx::QueryBuilder::new(
        "SELECT h.id, h.track_id, COALESCE(h.artist, t.artist) AS artist,
                COALESCE(h.title, t.title) AS title, t.album,
                CAST(UNIX_TIMESTAMP(h.started_at) * 1000 AS SIGNED) AS started_at_unix_ms,
                CAST(UNIX_TIMESTAMP(h.ended_at) * 1000 AS SIGNED) AS ended_at_unix_ms,
                h.played_ms, CAST(h.end_reason AS CHAR) AS end_reason,
                CAST(h.deck AS CHAR) AS deck, CAST(h.source AS CHAR) AS source
         FROM play_history h LEFT JOIN tracks t ON t.id = h.track_id WHERE TRUE",
    );
    if let Some(before) = query.before {
        sql.push(" AND h.started_at < FROM_UNIXTIME(")
            .push_bind(before as f64 / 1_000.0)
            .push(")");
    }
    if let Some(after) = query.after {
        sql.push(" AND h.started_at >= FROM_UNIXTIME(")
            .push_bind(after as f64 / 1_000.0)
            .push(")");
    }
    if let Some(track_id) = query.track_id {
        sql.push(" AND h.track_id = ").push_bind(track_id);
    }
    if query.exclude_fx {
        sql.push(" AND h.deck <> 'fx'");
    }
    sql.push(" ORDER BY h.started_at DESC, h.id DESC LIMIT ")
        .push_bind(limit);
    Ok(Json(sql.build_query_as().fetch_all(&state.pool).await?))
}
