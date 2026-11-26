// SPDX-License-Identifier: AGPL-3.0-or-later

//! The MusicBrainz identifiers of tracks, sent to other systems with the
//! track (webhooks, public endpoints): MusicBrainz's public identifiers,
//! never Heron's (docs/api.md#webhooks).

use std::collections::HashMap;

use serde::Serialize;
use sqlx::{MySqlPool, QueryBuilder};
use utoipa::ToSchema;

/// A track's MusicBrainz identifiers, as its tags and its MusicBrainz
/// details give them. Each is `null` when unknown.
#[derive(Clone, Debug, Default, PartialEq, Serialize, ToSchema, sqlx::FromRow)]
pub struct MusicBrainzIds {
    pub recording_id: Option<String>,
    pub release_id: Option<String>,
    pub release_group_id: Option<String>,
    /// The first credited artist.
    pub artist_id: Option<String>,
    /// The artist's Wikidata item (`Q42`).
    pub artist_wikidata_id: Option<String>,
    /// From the MusicBrainz details, else the year tag.
    pub first_release_year: Option<u16>,
}

/// The identifiers of `track_ids`, for the tracks that have at least one
/// MusicBrainz ID.
pub async fn load(
    pool: &MySqlPool,
    track_ids: &[u64],
) -> Result<HashMap<u64, MusicBrainzIds>, sqlx::Error> {
    if track_ids.is_empty() {
        return Ok(HashMap::new());
    }
    let mut query = QueryBuilder::new(
        "SELECT t.id, t.musicbrainz_recording_id AS recording_id,
                t.musicbrainz_release_id AS release_id,
                t.musicbrainz_release_group_id AS release_group_id,
                t.musicbrainz_artist_id AS artist_id, a.wikidata_id AS artist_wikidata_id,
                COALESCE(mb.first_release_year, t.year) AS first_release_year
         FROM tracks t
         LEFT JOIN track_musicbrainz mb ON mb.track_id = t.id
         LEFT JOIN musicbrainz_artists a ON a.mbid = t.musicbrainz_artist_id
         WHERE COALESCE(t.musicbrainz_recording_id, t.musicbrainz_release_id,
                        t.musicbrainz_release_group_id, t.musicbrainz_artist_id) IS NOT NULL
           AND t.id IN (",
    );
    let mut separated = query.separated(", ");
    for id in track_ids {
        separated.push_bind(*id);
    }
    query.push(")");
    let rows: Vec<Row> = query.build_query_as().fetch_all(pool).await?;
    Ok(rows.into_iter().map(|row| (row.id, row.ids)).collect())
}

/// The identifiers of one track, `None` without any (or when the database
/// cannot be read: the track matters more).
pub async fn of(pool: &MySqlPool, track_id: Option<u64>) -> Option<MusicBrainzIds> {
    let id = track_id?;
    match load(pool, &[id]).await {
        Ok(mut ids) => ids.remove(&id),
        Err(error) => {
            tracing::warn!(%error, "cannot read the MusicBrainz identifiers");
            None
        }
    }
}

#[derive(sqlx::FromRow)]
struct Row {
    id: u64,
    #[sqlx(flatten)]
    ids: MusicBrainzIds,
}
