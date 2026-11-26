// SPDX-License-Identifier: AGPL-3.0-or-later

//! Tracks imported as not on MusicBrainz (their own tags kept) are looked
//! up again by their title and artist every [`LOOK_AGAIN_DAYS`]: a track
//! too new for MusicBrainz is often added later. One found then waits on
//! *To do › To identify* with the releases found; the others wait again.

use sqlx::MySqlPool;

use crate::identify::{Identifier, matching::Candidate};

/// Days between two lookups of a track not on MusicBrainz.
pub const LOOK_AGAIN_DAYS: u32 = 90;

/// Tracks looked up in one pass, so the MusicBrainz rate limit is shared
/// with the rest.
pub const PER_PASS: u32 = 10;

/// A track whose turn has come: its ID, title, artist and album.
type Due = (u64, Option<String>, Option<String>, Option<String>);

/// What a pass did.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct LookAgainReport {
    pub looked_up: u64,
    pub found: u64,
}

/// Looks up the tracks whose turn has come. A lookup that fails is tried
/// again at the next pass.
pub async fn look_again(
    pool: &MySqlPool,
    identifier: &Identifier,
) -> Result<LookAgainReport, sqlx::Error> {
    let due: Vec<Due> = sqlx::query_as(
        "SELECT t.id, t.title, t.artist, t.album FROM track_identifications i
         JOIN tracks t ON t.id = i.track_id AND t.deleted_at IS NULL
         WHERE i.status = 'kept'
           AND COALESCE(i.look_again_at, i.updated_at + INTERVAL ? DAY) <= NOW(6)
         ORDER BY COALESCE(i.look_again_at, i.updated_at + INTERVAL ? DAY)
         LIMIT ?",
    )
    .bind(LOOK_AGAIN_DAYS)
    .bind(LOOK_AGAIN_DAYS)
    .bind(PER_PASS)
    .fetch_all(pool)
    .await?;
    let mut report = LookAgainReport::default();
    for (track_id, title, artist, album) in due {
        let (Some(title), Some(artist)) = (
            title.filter(|t| !t.trim().is_empty()),
            artist.filter(|a| !a.trim().is_empty()),
        ) else {
            later(pool, track_id).await?;
            continue;
        };
        match identifier
            .search_recordings(&title, &artist, album.as_deref(), &[])
            .await
        {
            Ok(found) if found.is_empty() => later(pool, track_id).await?,
            Ok(found) => {
                found_now(pool, track_id, &found).await?;
                report.found += 1;
            }
            Err(error) => {
                tracing::warn!(track_id, %error, "cannot look a track up again on MusicBrainz");
                break;
            }
        }
        report.looked_up += 1;
    }
    if report.found > 0 {
        tracing::info!(?report, "tracks now on MusicBrainz");
    }
    Ok(report)
}

/// Not there yet: looked up again in [`LOOK_AGAIN_DAYS`].
async fn later(pool: &MySqlPool, track_id: u64) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE track_identifications SET look_again_at = NOW(6) + INTERVAL ? DAY
         WHERE track_id = ?",
    )
    .bind(LOOK_AGAIN_DAYS)
    .bind(track_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// On MusicBrainz now: waits on *To identify* with the releases found.
async fn found_now(
    pool: &MySqlPool,
    track_id: u64,
    found: &[Candidate],
) -> Result<(), sqlx::Error> {
    let candidates = crate::import::candidates_json(found).map(|json| json.to_string());
    sqlx::query(
        "UPDATE track_identifications
         SET status = 'needs_review', reason = ?, score = ?, candidates = ?, look_again_at = NULL
         WHERE track_id = ? AND status = 'kept'",
    )
    .bind("on MusicBrainz now (it was not when imported): pick its release")
    .bind(found.iter().map(|c| c.score).reduce(f64::max))
    .bind(candidates)
    .bind(track_id)
    .execute(pool)
    .await?;
    Ok(())
}
