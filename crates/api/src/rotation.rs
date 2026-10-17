// SPDX-License-Identifier: AGPL-3.0-or-later

//! Rotation in the API: the station as `heron-rotation` sees it, loaded
//! from the database; the selection log; simulations.
//!
//! - The library holds the tracks rotation may pick: enabled, present,
//!   analyzed, with something audible between their cue points.
//! - The history is what played on the decks (the FX player aside) as far
//!   back as separation looks, followed by the queue at its planned times.

use std::collections::{HashMap, HashSet};
use std::time::SystemTime;

use axum::Json;
use axum::extract::{Query, State};
use heron_rotation::names::{normalize, title_key};
use heron_rotation::{
    Category, Clock, Explanation, History, Library, LibraryParts, Member, Method, Play, Position,
    Restriction, RotationSettings, Rule, Schedule, SeparationOverrides, Simulation, Slot, Target,
    Track,
};
use serde::{Deserialize, Serialize};
use sqlx::{MySqlPool, QueryBuilder};
use utoipa::{IntoParams, ToSchema};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::app::AppState;
use crate::auth::{CurrentUser, Role};
use crate::engine::{DeckName, PlayerState, QueueEntry, unix_ms};
use crate::error::{ApiError, ApiResult, ErrorBody};
use crate::settings::{self, StationSettings};

/// Playable length under which an analyzed track counts as silent.
pub const SILENT_MS: u32 = 1_000;

/// Planned length of a queued track whose length is unknown.
const UNKNOWN_LENGTH_MS: i64 = 210_000;

/// Overrides stored as JSON; invalid ones count as none.
pub fn overrides(json: Option<&str>) -> SeparationOverrides {
    json.and_then(|json| match serde_json::from_str(json) {
        Ok(overrides) => Some(overrides),
        Err(error) => {
            tracing::warn!(%error, "ignoring invalid separation overrides");
            None
        }
    })
    .unwrap_or_default()
}

pub fn target(kind: &str, id: u64) -> Option<Target> {
    match kind {
        "track" => Some(Target::Track(id)),
        "artist" => Some(Target::Artist(id)),
        "category" => Some(Target::Category(id)),
        _ => None,
    }
}

/// The station as rotation sees it.
pub async fn load_library(pool: &MySqlPool) -> ApiResult<Library> {
    let rows: Vec<(u64, Option<String>, Option<String>, i64)> = sqlx::query_as(
        "SELECT t.id, t.title, t.album,
                CAST(COALESCE(o.cue_out_ms, a.cue_out_ms) AS SIGNED)
                  - CAST(COALESCE(o.cue_in_ms, a.cue_in_ms) AS SIGNED)
         FROM tracks t
         JOIN track_analysis a ON a.track_id = t.id
         LEFT JOIN track_overrides o ON o.track_id = t.id
         WHERE t.enabled AND t.missing_since IS NULL
           AND COALESCE(o.cue_out_ms, a.cue_out_ms) >= COALESCE(o.cue_in_ms, a.cue_in_ms) + ?
         ORDER BY t.id",
    )
    .bind(SILENT_MS)
    .fetch_all(pool)
    .await?;
    let mut artists: HashMap<u64, Vec<u64>> = HashMap::new();
    let links: Vec<(u64, u64)> =
        sqlx::query_as("SELECT track_id, artist_id FROM track_artists ORDER BY track_id, position")
            .fetch_all(pool)
            .await?;
    for (track, artist) in links {
        artists.entry(track).or_default().push(artist);
    }
    let tracks = rows
        .into_iter()
        .map(|(id, title, album, length_ms)| Track {
            id,
            title_key: title_key(title.as_deref()),
            album_key: title_key(album.as_deref()),
            artists: artists.remove(&id).unwrap_or_default(),
            categories: Vec::new(),
            length_ms: u32::try_from(length_ms).unwrap_or(0),
        })
        .collect();

    let mut members: HashMap<u64, Vec<Member>> = HashMap::new();
    let rows: Vec<(u64, u64, f64)> = sqlx::query_as(
        "SELECT category_id, track_id, weight FROM track_categories ORDER BY category_id, track_id",
    )
    .fetch_all(pool)
    .await?;
    for (category, track, weight) in rows {
        members
            .entry(category)
            .or_default()
            .push(Member { track, weight });
    }
    let rows: Vec<(u64, String, Option<String>)> =
        sqlx::query_as("SELECT id, name, CAST(separation AS CHAR) FROM categories")
            .fetch_all(pool)
            .await?;
    let categories = rows
        .into_iter()
        .map(|(id, name, separation)| Category {
            id,
            name,
            separation: overrides(separation.as_deref()),
            members: members.remove(&id).unwrap_or_default(),
        })
        .collect();

    let mut slots: HashMap<u64, Vec<Slot>> = HashMap::new();
    let rows: Vec<(u64, u64, String, bool, Option<String>)> = sqlx::query_as(
        "SELECT clock_id, category_id, CAST(selection_method AS CHAR), enforce_rules,
                CAST(separation AS CHAR)
         FROM clock_slots ORDER BY clock_id, position",
    )
    .fetch_all(pool)
    .await?;
    for (clock, category, method, enforce_rules, separation) in rows {
        slots.entry(clock).or_default().push(Slot {
            category,
            method: Method::parse(&method).unwrap_or_default(),
            enforce_rules,
            separation: overrides(separation.as_deref()),
        });
    }
    let rows: Vec<(u64, String, bool)> = sqlx::query_as("SELECT id, name, looping FROM clocks")
        .fetch_all(pool)
        .await?;
    let clocks = rows
        .into_iter()
        .map(|(id, name, looping)| Clock {
            id,
            name,
            slots: slots.remove(&id).unwrap_or_default(),
            looping,
        })
        .collect();
    let rows: Vec<(u8, u8, u64)> =
        sqlx::query_as("SELECT day_of_week, hour, clock_id FROM clock_schedule")
            .fetch_all(pool)
            .await?;
    let schedule = Schedule {
        hours: rows
            .into_iter()
            .map(|(day, hour, clock)| ((day, hour), clock))
            .collect(),
    };

    let rows: Vec<(u64, String, u64, String)> = sqlx::query_as(
        "SELECT id, CAST(target AS CHAR), target_id, CAST(rule AS CHAR)
         FROM restrictions WHERE enabled ORDER BY id",
    )
    .fetch_all(pool)
    .await?;
    let mut restrictions = Vec::with_capacity(rows.len());
    for (id, kind, target_id, rule) in rows {
        let rule: Rule = serde_json::from_str(&rule)
            .map_err(|error| ApiError::Internal(format!("restriction {id}: {error}")))?;
        let target = target(&kind, target_id)
            .ok_or_else(|| ApiError::Internal(format!("restriction {id}: target {kind}")))?;
        restrictions.push(Restriction { id, target, rule });
    }
    let similar: Vec<(u64, u64)> =
        sqlx::query_as("SELECT artist_id, similar_id FROM artist_similar")
            .fetch_all(pool)
            .await?;
    let settings: RotationSettings = settings::load(pool, RotationSettings::SETTINGS_NAME).await?;
    let station: StationSettings = settings::load(pool, StationSettings::SETTINGS_NAME).await?;
    Ok(Library::new(
        LibraryParts {
            tracks,
            categories,
            similar,
            restrictions,
            clocks,
            schedule,
            settings,
        },
        station.time_zone(),
    ))
}

/// A play as recorded, or a queued track.
#[derive(sqlx::FromRow)]
struct Recorded {
    track_id: Option<u64>,
    artist: Option<String>,
    title: Option<String>,
    album: Option<String>,
    start_ms: i64,
}

/// Plays for rotation; tracks rotation cannot pick (disabled, deleted,
/// outside the library) are described from the database or their tags.
async fn plays(
    pool: &MySqlPool,
    library: &Library,
    recorded: Vec<Recorded>,
) -> ApiResult<Vec<Play>> {
    let others: Vec<u64> = recorded
        .iter()
        .filter_map(|r| r.track_id)
        .filter(|id| !library.tracks.contains_key(id))
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    let mut known: HashMap<u64, (Vec<u64>, Vec<u64>)> = HashMap::new();
    if !others.is_empty() {
        let mut query = QueryBuilder::new(
            "SELECT track_id, artist_id, 0 FROM track_artists WHERE track_id IN (",
        );
        let mut ids = query.separated(", ");
        for id in &others {
            ids.push_bind(*id);
        }
        query.push(
            ") UNION ALL SELECT track_id, category_id, 1 FROM track_categories WHERE track_id IN (",
        );
        let mut ids = query.separated(", ");
        for id in &others {
            ids.push_bind(*id);
        }
        query.push(")");
        let rows: Vec<(u64, u64, i32)> = query.build_query_as().fetch_all(pool).await?;
        for (track, id, kind) in rows {
            let entry = known.entry(track).or_default();
            if kind == 0 {
                entry.0.push(id);
            } else {
                entry.1.push(id);
            }
        }
    }
    // Plays without a library track: their artists by name.
    let names: HashSet<String> = recorded
        .iter()
        .filter(|r| {
            r.track_id
                .is_none_or(|id| !known.contains_key(&id) && !library.tracks.contains_key(&id))
        })
        .flat_map(|r| heron_rotation::names::artist_names(r.artist.as_deref(), r.title.as_deref()))
        .map(|name| normalize(&name))
        .collect();
    let mut by_name: HashMap<String, u64> = HashMap::new();
    if !names.is_empty() {
        let mut query = QueryBuilder::new(
            "SELECT normalized, artist_id FROM artist_names WHERE normalized_hash IN (",
        );
        let mut hashes = query.separated(", ");
        for name in &names {
            hashes
                .push("UNHEX(SHA2(")
                .push_bind_unseparated(name)
                .push_unseparated(", 256))");
        }
        query.push(")");
        let rows: Vec<(String, u64)> = query.build_query_as().fetch_all(pool).await?;
        by_name.extend(rows);
    }
    Ok(recorded
        .into_iter()
        .map(|r| {
            if let Some(track) = r.track_id.and_then(|id| library.tracks.get(&id)) {
                return Play::of(track, r.start_ms);
            }
            let (artists, categories) = match r.track_id.and_then(|id| known.get(&id).cloned()) {
                Some(found) => found,
                None => (
                    heron_rotation::names::artist_names(r.artist.as_deref(), r.title.as_deref())
                        .iter()
                        .filter_map(|name| by_name.get(&normalize(name)).copied())
                        .collect(),
                    Vec::new(),
                ),
            };
            Play {
                track: r.track_id,
                artists,
                title_key: title_key(r.title.as_deref()),
                album_key: title_key(r.album.as_deref()),
                categories,
                start_ms: r.start_ms,
            }
        })
        .collect())
}

/// What played up to `now_ms`, as far back as separation looks, and when
/// each track last played.
pub async fn load_history(pool: &MySqlPool, library: &Library, now_ms: i64) -> ApiResult<History> {
    let since_ms = now_ms - library.horizon_ms();
    let recorded: Vec<Recorded> = sqlx::query_as(
        "SELECT h.track_id, COALESCE(h.artist, t.artist) AS artist,
                COALESCE(h.title, t.title) AS title, t.album,
                CAST(UNIX_TIMESTAMP(h.started_at) * 1000 AS SIGNED) AS start_ms
         FROM play_history h LEFT JOIN tracks t ON t.id = h.track_id
         WHERE h.deck <> 'fx' AND h.started_at >= FROM_UNIXTIME(?)
         ORDER BY h.started_at, h.id",
    )
    .bind(since_ms as f64 / 1_000.0)
    .fetch_all(pool)
    .await?;
    let plays = plays(pool, library, recorded).await?;
    let last: Vec<(u64, i64)> = sqlx::query_as(
        "SELECT track_id, CAST(UNIX_TIMESTAMP(MAX(started_at)) * 1000 AS SIGNED)
         FROM play_history WHERE track_id IS NOT NULL AND deck <> 'fx' GROUP BY track_id",
    )
    .fetch_all(pool)
    .await?;
    Ok(History::new(plays, last.into_iter().collect()))
}

/// Where the clock is: the slot of the last pick made from a clock.
pub async fn last_position(pool: &MySqlPool) -> ApiResult<Option<Position>> {
    let row: Option<(u64, u32, i64)> = sqlx::query_as(
        "SELECT clock_id, slot, CAST(UNIX_TIMESTAMP(hour_start) * 1000 AS SIGNED)
         FROM rotation_picks
         WHERE clock_id IS NOT NULL AND slot IS NOT NULL AND hour_start IS NOT NULL
         ORDER BY id DESC LIMIT 1",
    )
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|(clock_id, slot, hour_start_unix_ms)| Position {
        hour_start_unix_ms,
        clock_id,
        slot,
    }))
}

fn planned_length_ms(entry: &QueueEntry) -> i64 {
    let track = &entry.track;
    let end = if track.cue_out_ms > track.cue_in_ms {
        track.cue_out_ms
    } else {
        track.duration_ms
    };
    match i64::from(end) - i64::from(track.cue_in_ms) {
        length if length > 0 => length,
        _ => UNKNOWN_LENGTH_MS,
    }
}

/// When each waiting track should start, and when a track added after
/// them would: what the decks still have to play, then the queue in order.
pub fn planned_starts(state: &PlayerState, now_ms: i64) -> (Vec<(&QueueEntry, i64)>, i64) {
    let remaining = state
        .decks
        .iter()
        .filter(|deck| deck.deck != DeckName::Fx && deck.entry.is_some())
        .map(|deck| i64::try_from(deck.length_ms.saturating_sub(deck.position_ms)).unwrap_or(0))
        .max()
        .unwrap_or(0);
    let mut at = now_ms + remaining;
    let mut planned = Vec::new();
    for entry in state.waiting() {
        planned.push((entry, at));
        at += planned_length_ms(entry);
    }
    (planned, at)
}

/// The history with the waiting tracks added at their planned times.
pub async fn with_queue(
    pool: &MySqlPool,
    library: &Library,
    mut history: History,
    planned: &[(&QueueEntry, i64)],
) -> ApiResult<History> {
    let recorded = planned
        .iter()
        .map(|(entry, at)| Recorded {
            track_id: entry.track.track_id,
            artist: entry.track.artist.clone(),
            title: entry.track.title.clone(),
            album: entry.track.album.clone(),
            start_ms: *at,
        })
        .collect();
    for play in plays(pool, library, recorded).await? {
        history.push(play);
    }
    Ok(history)
}

pub fn now_ms() -> i64 {
    i64::try_from(unix_ms(SystemTime::now())).unwrap_or(i64::MAX)
}

/// A random seed.
pub fn seed() -> u64 {
    let mut bytes = [0; 8];
    if getrandom::fill(&mut bytes).is_err() {
        return now_ms() as u64;
    }
    u64::from_le_bytes(bytes)
}

/// Records a pick in the selection log. `entry` is the engine's queue
/// entry it became, with the engine's start time.
pub async fn record_pick(
    pool: &MySqlPool,
    pick: &heron_rotation::Pick,
    entry: Option<(u64, u64)>,
) -> ApiResult<u64> {
    let explanation = serde_json::to_string(&pick.explanation)
        .map_err(|error| ApiError::Internal(error.to_string()))?;
    let position = pick.position;
    let id = sqlx::query(
        "INSERT INTO rotation_picks
             (track_id, planned_at, clock_id, slot, hour_start, category_id, step, explanation,
              engine_started_at, entry_id)
         VALUES (?, FROM_UNIXTIME(?), ?, ?, FROM_UNIXTIME(?), ?, ?, ?, ?, ?)",
    )
    .bind(pick.track)
    .bind(pick.explanation.at_unix_ms as f64 / 1_000.0)
    .bind(position.map(|p| p.clock_id))
    .bind(position.map(|p| p.slot))
    .bind(position.map(|p| p.hour_start_unix_ms as f64 / 1_000.0))
    .bind(pick.explanation.category_id)
    .bind(pick.explanation.step)
    .bind(explanation)
    .bind(entry.map(|(started_at, _)| started_at))
    .bind(entry.map(|(_, entry_id)| entry_id))
    .execute(pool)
    .await?
    .last_insert_id();
    if pick.explanation.step > 0 {
        tracing::info!(
            track = pick.track,
            step = pick.explanation.step,
            relaxed = %pick.explanation.step_description,
            category = ?pick.explanation.slot_category_id,
            "rotation relaxed its rules"
        );
    }
    Ok(id)
}

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_picks))
        .routes(routes!(simulate))
}

/// A track rotation queued, and why.
#[derive(Debug, Serialize, ToSchema)]
pub struct RotationPick {
    pub id: u64,
    /// Missing once the track is deleted.
    pub track_id: Option<u64>,
    pub artist: Option<String>,
    pub title: Option<String>,
    pub planned_at_unix_ms: i64,
    /// When it started; missing until then, or if it never played.
    pub played_at_unix_ms: Option<i64>,
    pub explanation: Explanation,
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct PicksQuery {
    /// Picks before this one, for the next page.
    pub before: Option<u64>,
    /// Only this track.
    pub track_id: Option<u64>,
    /// 1 to 200, 50 by default.
    pub limit: Option<u32>,
}

/// The selection log, most recent first.
#[utoipa::path(get, path = "/rotation/picks", tag = "rotation",
    params(PicksQuery),
    responses((status = 200, body = Vec<RotationPick>), (status = "4XX", body = ErrorBody)))]
async fn list_picks(
    _user: CurrentUser,
    State(state): State<AppState>,
    Query(query): Query<PicksQuery>,
) -> ApiResult<Json<Vec<RotationPick>>> {
    let limit = query.limit.unwrap_or(50);
    if !(1..=200).contains(&limit) {
        return Err(ApiError::BadRequest("limit is 1 to 200".to_owned()));
    }
    let mut sql = QueryBuilder::new(
        "SELECT p.id, p.track_id, t.artist, t.title,
                CAST(UNIX_TIMESTAMP(p.planned_at) * 1000 AS SIGNED),
                CAST(UNIX_TIMESTAMP(h.started_at) * 1000 AS SIGNED),
                CAST(p.explanation AS CHAR)
         FROM rotation_picks p
         LEFT JOIN tracks t ON t.id = p.track_id
         LEFT JOIN play_history h ON h.id = p.play_id
         WHERE TRUE",
    );
    if let Some(before) = query.before {
        sql.push(" AND p.id < ").push_bind(before);
    }
    if let Some(track_id) = query.track_id {
        sql.push(" AND p.track_id = ").push_bind(track_id);
    }
    sql.push(" ORDER BY p.id DESC LIMIT ").push_bind(limit);
    type Row = (
        u64,
        Option<u64>,
        Option<String>,
        Option<String>,
        i64,
        Option<i64>,
        String,
    );
    let rows: Vec<Row> = sql.build_query_as().fetch_all(&state.pool).await?;
    rows.into_iter()
        .map(
            |(id, track_id, artist, title, planned, played, explanation)| {
                Ok(RotationPick {
                    id,
                    track_id,
                    artist,
                    title,
                    planned_at_unix_ms: planned,
                    played_at_unix_ms: played,
                    explanation: serde_json::from_str(&explanation)
                        .map_err(|error| ApiError::Internal(format!("pick {id}: {error}")))?,
                })
            },
        )
        .collect::<ApiResult<_>>()
        .map(Json)
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct SimulationRequest {
    /// 1 to 168 (a week, the default).
    pub hours: Option<u32>,
    /// Now by default, after the plays so far.
    pub start_unix_ms: Option<i64>,
    /// To replay a simulation; random by default.
    pub seed: Option<u64>,
    /// Runs this clock at every hour instead of the schedule.
    pub clock_id: Option<u64>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct TrackLabel {
    pub id: u64,
    pub artist: Option<String>,
    pub title: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SimulationReport {
    pub seed: u64,
    pub simulation: Simulation,
    /// The tracks picked.
    pub tracks: Vec<TrackLabel>,
}

/// Generates programming offline, without touching the queue, and checks
/// it: separation as relaxed at each pick's step, restrictions always.
/// Operators and admins.
#[utoipa::path(post, path = "/rotation/simulate", tag = "rotation",
    request_body = SimulationRequest,
    responses((status = 200, body = SimulationReport), (status = "4XX", body = ErrorBody)))]
async fn simulate(
    user: CurrentUser,
    State(state): State<AppState>,
    Json(request): Json<SimulationRequest>,
) -> ApiResult<Json<SimulationReport>> {
    user.require(Role::Operator)?;
    let hours = request.hours.unwrap_or(168);
    if !(1..=168).contains(&hours) {
        return Err(ApiError::Invalid("hours is 1 to 168".to_owned()));
    }
    let mut library = load_library(&state.pool).await?;
    if let Some(clock_id) = request.clock_id {
        if !library.clocks.contains_key(&clock_id) {
            return Err(ApiError::not_found("clock"));
        }
        library.schedule.hours = (1..=7)
            .flat_map(|day| (0..24).map(move |hour| ((day, hour), clock_id)))
            .collect();
    }
    let now = now_ms();
    let start = request.start_unix_ms.unwrap_or(now);
    let history = load_history(&state.pool, &library, start.min(now)).await?;
    let last = last_position(&state.pool).await?;
    let seed = request.seed.unwrap_or_else(seed);
    let duration = i64::from(hours) * 3_600_000;
    let simulation = tokio::task::spawn_blocking(move || {
        heron_rotation::simulate(&library, history, last, start, duration, seed)
    })
    .await
    .map_err(|error| ApiError::Internal(error.to_string()))?;
    let ids: Vec<u64> = simulation
        .picks
        .iter()
        .map(|pick| pick.track_id)
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    let mut tracks = Vec::new();
    if !ids.is_empty() {
        let mut query = QueryBuilder::new("SELECT id, artist, title FROM tracks WHERE id IN (");
        let mut separated = query.separated(", ");
        for id in &ids {
            separated.push_bind(*id);
        }
        query.push(") ORDER BY id");
        let rows: Vec<(u64, Option<String>, Option<String>)> =
            query.build_query_as().fetch_all(&state.pool).await?;
        tracks = rows
            .into_iter()
            .map(|(id, artist, title)| TrackLabel { id, artist, title })
            .collect();
    }
    tracing::info!(
        user = %user.username,
        hours,
        picks = simulation.picks.len(),
        violations = simulation.violations.len(),
        "rotation simulated"
    );
    Ok(Json(SimulationReport {
        seed,
        simulation,
        tracks,
    }))
}
