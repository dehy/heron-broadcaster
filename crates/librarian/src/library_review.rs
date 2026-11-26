// SPDX-License-Identifier: AGPL-3.0-or-later

//! The library reviewed like the buffer zone:
//!
//! - [`check_files`]: every track's file is checked for damage
//!   ([`heron_library::integrity`]), and again when it changes.
//! - [`identify_library`]: tracks never identified, without a MusicBrainz
//!   recording, are identified by their stored fingerprint (with the
//!   station's AcoustID key), else searched by their title and artist.
//!   Nothing is written: each waits on *To do › To identify*, a confident
//!   match ready to be accepted.
//! - [`search_track`]: a track searched by hand, from the review.

use heron_analysis::Chromaprint;
use heron_core::LibraryPath;
use heron_library::integrity::{self, Damage};
use sqlx::MySqlPool;

use crate::identify::matching::Candidate;
use crate::identify::{FileToIdentify, Identified, Identifier, IdentifyError};
use crate::journal::Roots;

/// The flag each damage is stored and shown as.
fn flag(damage: Damage) -> &'static str {
    match damage {
        Damage::SeekTable => "broken_seek_table",
        Damage::Frames { .. } => "damaged_audio",
        Damage::Mixed { .. } => "mixed_streams",
    }
}

/// Checks up to `limit` tracks never checked, or whose file changed since.
/// Returns how many were checked.
pub async fn check_files(pool: &MySqlPool, roots: &Roots, limit: u32) -> Result<u64, sqlx::Error> {
    let due: Vec<(u64, String, Option<Vec<u8>>)> = sqlx::query_as(
        "SELECT t.id, t.path, t.file_hash FROM tracks t
         LEFT JOIN track_integrity i ON i.track_id = t.id
         WHERE t.deleted_at IS NULL AND t.missing_since IS NULL
           AND (i.track_id IS NULL OR NOT (i.file_hash <=> t.file_hash))
         ORDER BY t.id LIMIT ?",
    )
    .bind(limit)
    .fetch_all(pool)
    .await?;
    let mut checked = 0;
    for (track_id, path, hash) in due {
        let file = LibraryPath::from_stored(path).resolve(&roots.library);
        let found = tokio::task::spawn_blocking(move || integrity::check(&file))
            .await
            .unwrap_or_default();
        let damage = (!found.is_empty()).then(|| {
            let mut flags: Vec<&str> = found.into_iter().map(flag).collect();
            flags.dedup();
            serde_json::to_string(&flags).unwrap_or_default()
        });
        if damage.is_some() {
            tracing::info!(
                track_id,
                damage = damage.as_deref(),
                "damaged file in the library"
            );
        }
        sqlx::query(
            "INSERT INTO track_integrity (track_id, file_hash, damage, checked_at)
             VALUES (?, ?, ?, NOW(6))
             ON DUPLICATE KEY UPDATE file_hash = VALUES(file_hash), damage = VALUES(damage),
                 checked_at = VALUES(checked_at)",
        )
        .bind(track_id)
        .bind(hash)
        .bind(damage)
        .execute(pool)
        .await?;
        checked += 1;
    }
    Ok(checked)
}

/// A track never identified: its ID, tags, length and fingerprint.
type Unidentified = (
    u64,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<u32>,
    Option<Vec<u8>>,
);

/// Identifies up to `limit` tracks never identified. `by_sound` is set
/// with the station's AcoustID key. Stops at the first lookup that fails,
/// to try again later.
pub async fn identify_library(
    pool: &MySqlPool,
    by_sound: Option<&Identifier>,
    searcher: &Identifier,
    limit: u32,
) -> Result<u64, sqlx::Error> {
    let due: Vec<Unidentified> = sqlx::query_as(
        "SELECT t.id, t.title, t.artist, t.album, t.duration_ms, f.chromaprint FROM tracks t
         LEFT JOIN track_identifications i ON i.track_id = t.id
         LEFT JOIN track_fingerprints f ON f.track_id = t.id
         WHERE t.deleted_at IS NULL AND i.track_id IS NULL
           AND t.musicbrainz_recording_id IS NULL
         ORDER BY t.id LIMIT ?",
    )
    .bind(limit)
    .fetch_all(pool)
    .await?;
    let mut done = 0;
    for (track_id, title, artist, album, duration_ms, chromaprint) in due {
        let mut found = None;
        if let (Some(identifier), Some(print)) = (by_sound, chromaprint) {
            let file = FileToIdentify {
                chromaprint: Some(Chromaprint::from_compressed(print)),
                duration_s: duration_ms.map(|ms| (ms + 500) / 1000),
                album_tag: album.clone(),
            };
            match identifier.identify(&[file]).await {
                Ok(mut identified) => found = identified.pop(),
                Err(error) => {
                    tracing::warn!(track_id, %error, "cannot identify a library track");
                    break;
                }
            }
        }
        let row = match found {
            Some(Identified::Match {
                score,
                recording_mbid,
                release_mbid,
                candidates,
                ..
            }) => Row {
                reason: format!(
                    "found by its sound (AcoustID {:.0}%): accept it",
                    score * 100.0
                ),
                score: Some(score),
                recording: Some(recording_mbid),
                release: Some(release_mbid),
                candidates,
            },
            Some(Identified::Review { reason, candidates }) if !candidates.is_empty() => Row {
                score: candidates.iter().map(|c| c.score).reduce(f64::max),
                reason,
                recording: None,
                release: None,
                candidates,
            },
            _ => match by_tags(
                searcher,
                title.as_deref(),
                artist.as_deref(),
                album.as_deref(),
                &[],
            )
            .await
            {
                Ok(row) => row,
                Err(error) => {
                    tracing::warn!(track_id, %error, "cannot search MusicBrainz for a library track");
                    break;
                }
            },
        };
        save(pool, track_id, &row).await?;
        done += 1;
    }
    if done > 0 {
        tracing::info!(
            tracks = done,
            "library tracks identified, waiting on To identify"
        );
    }
    Ok(done)
}

/// What an identification found, as `track_identifications` keeps it.
struct Row {
    reason: String,
    score: Option<f64>,
    recording: Option<String>,
    release: Option<String>,
    candidates: Vec<Candidate>,
}

async fn by_tags(
    searcher: &Identifier,
    title: Option<&str>,
    artist: Option<&str>,
    album: Option<&str>,
    kinds: &[heron_core::library::AlbumKind],
) -> Result<Row, IdentifyError> {
    let (Some(title), Some(artist)) = (
        title.map(str::trim).filter(|t| !t.is_empty()),
        artist.map(str::trim).filter(|a| !a.is_empty()),
    ) else {
        return Ok(Row {
            reason: "no title or artist to search MusicBrainz with".to_owned(),
            score: None,
            recording: None,
            release: None,
            candidates: Vec::new(),
        });
    };
    let candidates = searcher
        .search_recordings(title, artist, album, kinds)
        .await?;
    Ok(Row {
        reason: if candidates.is_empty() {
            "not found on MusicBrainz by its title and artist".to_owned()
        } else {
            "found on MusicBrainz by its title and artist: pick its release".to_owned()
        },
        score: candidates.iter().map(|c| c.score).reduce(f64::max),
        recording: None,
        release: None,
        candidates,
    })
}

async fn save(pool: &MySqlPool, track_id: u64, row: &Row) -> Result<(), sqlx::Error> {
    let candidates = crate::import::candidates_json(&row.candidates).map(|json| json.to_string());
    sqlx::query(
        "INSERT INTO track_identifications
             (track_id, status, reason, score, recording_mbid, release_mbid, candidates)
         VALUES (?, 'needs_review', ?, ?, ?, ?, ?)
         ON DUPLICATE KEY UPDATE status = 'needs_review', reason = VALUES(reason),
             score = VALUES(score), recording_mbid = VALUES(recording_mbid),
             release_mbid = VALUES(release_mbid), candidates = VALUES(candidates),
             look_again_at = NULL",
    )
    .bind(track_id)
    .bind(&row.reason)
    .bind(row.score)
    .bind(&row.recording)
    .bind(&row.release)
    .bind(candidates)
    .execute(pool)
    .await?;
    Ok(())
}

/// Searches MusicBrainz for a track by the words given: what it finds
/// replaces its candidates on *To identify*.
pub async fn search_track(
    pool: &MySqlPool,
    searcher: &Identifier,
    track_id: u64,
    title: &str,
    artist: &str,
    album: Option<&str>,
    kinds: &[heron_core::library::AlbumKind],
) -> Result<serde_json::Value, String> {
    let row = by_tags(searcher, Some(title), Some(artist), album, kinds)
        .await
        .map_err(|error| format!("cannot search MusicBrainz: {error}"))?;
    save(pool, track_id, &row)
        .await
        .map_err(|error| error.to_string())?;
    Ok(serde_json::json!({ "candidates": row.candidates.len() }))
}
