// SPDX-License-Identifier: AGPL-3.0-or-later

//! Programming details from MusicBrainz for identified tracks (those whose
//! tags carry a MusicBrainz recording and release): album, record label
//! and catalog number, ISRC, first release year and genres. One request per
//! release, at MusicBrainz's pace (one a second). Each MusicBrainz artist
//! the tracks carry is looked up too, once, for its Wikidata item.
//!
//! The whole library is looked up on request; after the first request,
//! tracks that have none yet (new tracks) are looked up in the background.

use std::collections::BTreeMap;

use heron_core::library::DetailsReport;
use sqlx::MySqlPool;

use crate::identify::{Identifier, RecordingDetails};

/// Releases looked up per background pass, so tasks asked meanwhile wait
/// at most this many seconds.
pub const BACKGROUND_RELEASES: usize = 30;

/// Looks up the tracks whose details were never fetched (or were fetched
/// for another release), or every identified track with `all`; at most
/// `limit` releases.
pub async fn look_up(
    pool: &MySqlPool,
    identifier: &Identifier,
    all: bool,
    limit: Option<usize>,
) -> Result<DetailsReport, sqlx::Error> {
    let stale = if all {
        ""
    } else {
        " AND (mb.track_id IS NULL OR mb.release_id <> t.musicbrainz_release_id
               OR mb.recording_id <> t.musicbrainz_recording_id)"
    };
    let rows: Vec<(u64, String, String)> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT t.id, t.musicbrainz_release_id, t.musicbrainz_recording_id
         FROM tracks t LEFT JOIN track_musicbrainz mb ON mb.track_id = t.id
         WHERE t.musicbrainz_release_id IS NOT NULL AND t.deleted_at IS NULL
           AND t.musicbrainz_recording_id IS NOT NULL{stale}
         ORDER BY t.musicbrainz_release_id, t.id"
    )))
    .fetch_all(pool)
    .await?;
    let mut releases: BTreeMap<String, Vec<(u64, String)>> = BTreeMap::new();
    for (track_id, release, recording) in rows {
        releases
            .entry(release)
            .or_default()
            .push((track_id, recording));
    }
    let mut report = DetailsReport::default();
    for (release, tracks) in releases.iter().take(limit.unwrap_or(usize::MAX)) {
        report.releases += 1;
        let recordings = match identifier.release_details(release).await {
            Ok(recordings) => recordings.unwrap_or_default(),
            Err(error) => {
                tracing::warn!(release, %error, "cannot look up the release on MusicBrainz");
                report.failed += 1;
                continue;
            }
        };
        for (track_id, recording) in tracks {
            // Unknown to MusicBrainz: saved empty, not looked up again.
            let details = recordings.get(recording).cloned().unwrap_or_default();
            save(pool, *track_id, release, recording, &details).await?;
            report.tracks += 1;
        }
    }
    let left = limit.map(|limit| limit.saturating_sub(releases.len().min(limit)));
    look_up_artists(pool, identifier, all, left, &mut report).await?;
    if report.releases > 0 || report.artists > 0 {
        tracing::info!(?report, "MusicBrainz details fetched");
    }
    Ok(report)
}

/// Looks up the Wikidata item of the MusicBrainz artists never looked up,
/// or of every artist with `all`; at most `limit`. An artist without a
/// Wikidata link is saved without one, and not looked up again.
async fn look_up_artists(
    pool: &MySqlPool,
    identifier: &Identifier,
    all: bool,
    limit: Option<usize>,
    report: &mut DetailsReport,
) -> Result<(), sqlx::Error> {
    let stale = if all { "" } else { " AND a.mbid IS NULL" };
    let artists: Vec<String> = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "SELECT DISTINCT t.musicbrainz_artist_id
         FROM tracks t LEFT JOIN musicbrainz_artists a ON a.mbid = t.musicbrainz_artist_id
         WHERE t.musicbrainz_artist_id IS NOT NULL AND t.deleted_at IS NULL{stale}
         ORDER BY t.musicbrainz_artist_id"
    )))
    .fetch_all(pool)
    .await?;
    // Only MusicBrainz IDs go into a request's address.
    let artists = artists.iter().filter(|id| is_mbid(id));
    for artist in artists.take(limit.unwrap_or(usize::MAX)) {
        report.artists += 1;
        let wikidata = match identifier.artist_wikidata(artist).await {
            Ok(wikidata) => wikidata,
            Err(error) => {
                tracing::warn!(artist, %error, "cannot look up the artist on MusicBrainz");
                report.failed += 1;
                continue;
            }
        };
        sqlx::query(
            "INSERT INTO musicbrainz_artists (mbid, wikidata_id, fetched_at)
             VALUES (?, ?, NOW(6))
             ON DUPLICATE KEY UPDATE wikidata_id = VALUES(wikidata_id),
                 fetched_at = VALUES(fetched_at)",
        )
        .bind(artist)
        .bind(wikidata)
        .execute(pool)
        .await?;
    }
    Ok(())
}

/// A MusicBrainz ID: 36 characters, hexadecimal digits and dashes.
fn is_mbid(id: &str) -> bool {
    id.len() == 36 && id.bytes().all(|b| b.is_ascii_hexdigit() || b == b'-')
}

/// Whether tracks are looked up in the background: once the station asked
/// for the whole library.
pub async fn background_enabled(pool: &MySqlPool) -> Result<bool, sqlx::Error> {
    let asked: Option<i32> = sqlx::query_scalar(
        "SELECT 1 FROM library_tasks
         WHERE kind = 'musicbrainz_details' AND status = 'done' LIMIT 1",
    )
    .fetch_optional(pool)
    .await?;
    Ok(asked.is_some())
}

async fn save(
    pool: &MySqlPool,
    track_id: u64,
    release: &str,
    recording: &str,
    details: &RecordingDetails,
) -> Result<(), sqlx::Error> {
    let genres = (!details.genres.is_empty())
        .then(|| serde_json::to_string(&details.genres).ok())
        .flatten();
    sqlx::query(
        "INSERT INTO track_musicbrainz
             (track_id, release_id, recording_id, album, record_label, catalog_number, isrc,
              first_release_year, genres, fetched_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, NOW(6))
         ON DUPLICATE KEY UPDATE release_id = VALUES(release_id),
             recording_id = VALUES(recording_id), album = VALUES(album),
             record_label = VALUES(record_label), catalog_number = VALUES(catalog_number),
             isrc = VALUES(isrc), first_release_year = VALUES(first_release_year),
             genres = VALUES(genres), fetched_at = VALUES(fetched_at)",
    )
    .bind(track_id)
    .bind(release)
    .bind(recording)
    .bind(&details.album)
    .bind(&details.record_label)
    .bind(&details.catalog_number)
    .bind(&details.isrc)
    .bind(details.first_release_year)
    .bind(genres)
    .execute(pool)
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn only_musicbrainz_ids_are_looked_up() {
        assert!(super::is_mbid("0383dadf-2a4e-4d10-a46a-e9e041da8eb3"));
        assert!(!super::is_mbid("0383dadf-2a4e-4d10-a46a-e9e041da8eb"));
        assert!(!super::is_mbid("../../release/x?inc=all&fmt=json&abcdef"));
    }
}
