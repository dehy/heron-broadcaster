// SPDX-License-Identifier: AGPL-3.0-or-later

//! MusicBrainz relationships between the station's artists ("member of
//! band", "is person", …), offered as suggestions for review: separation
//! then treats the artists as the same one if the suggestion is accepted.

use std::collections::HashMap;

use heron_core::library::RelationsReport;
use sqlx::MySqlPool;

use crate::identify::Identifier;

/// Relationships that make two artists sound like one to listeners.
const RELATIONS: [&str; 4] = ["member of band", "is person", "subgroup", "collaboration"];

/// Looks up every artist with a MusicBrainz ID (one request a second) and
/// suggests the links between artists of the station.
pub async fn look_up(
    pool: &MySqlPool,
    identifier: &Identifier,
) -> Result<RelationsReport, sqlx::Error> {
    let artists: Vec<(u64, String, String)> = sqlx::query_as(
        "SELECT id, name, musicbrainz_id FROM artists
         WHERE musicbrainz_id IS NOT NULL ORDER BY id",
    )
    .fetch_all(pool)
    .await?;
    let by_mbid: HashMap<&str, (u64, &str)> = artists
        .iter()
        .map(|(id, name, mbid)| (mbid.as_str(), (*id, name.as_str())))
        .collect();
    let mut report = RelationsReport::default();
    for (id, name, mbid) in &artists {
        report.artists += 1;
        let relations = match identifier.artist_relations(mbid).await {
            Ok(relations) => relations,
            Err(error) => {
                tracing::warn!(artist = id, %error, "cannot look up MusicBrainz relationships");
                report.failed += 1;
                continue;
            }
        };
        for (kind, other_mbid, backward) in relations {
            if !RELATIONS.contains(&kind.as_str()) {
                continue;
            }
            let Some(&(other, other_name)) = by_mbid.get(other_mbid.as_str()) else {
                continue;
            };
            if other == *id {
                continue;
            }
            let (subject, object) = if backward {
                (other_name, name.as_str())
            } else {
                (name.as_str(), other_name)
            };
            let added = sqlx::query(
                "INSERT IGNORE INTO artist_suggestions (artist_id, other_id, source, reason)
                 VALUES (?, ?, 'musicbrainz', ?)",
            )
            .bind(other.min(*id))
            .bind(other.max(*id))
            .bind(format!("{subject}: {kind} {object}"))
            .execute(pool)
            .await?
            .rows_affected();
            report.suggestions += added;
        }
    }
    Ok(report)
}
