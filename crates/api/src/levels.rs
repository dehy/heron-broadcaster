// SPDX-License-Identifier: AGPL-3.0-or-later

//! Each track's lowest and highest peak between its cue points, in dBFS
//! (docs/analysis.md#job-queue), from its waveform: computed again when the
//! track is analyzed again or its cue points change.

use std::time::Duration;

use heron_analysis::WAVEFORM_FLOOR_DB;
use sqlx::MySqlPool;

use crate::error::ApiResult;

/// How often tracks are looked at.
const INTERVAL: Duration = Duration::from_secs(5);
/// Tracks computed in one go.
const BATCH: u32 = 200;
/// No cue-out: to the end.
const END: u32 = u32::MAX;

/// A waveform byte in dBFS: 0 is the floor (−72) or below, 255 full scale.
pub fn byte_db(byte: u8) -> f64 {
    let floor = f64::from(WAVEFORM_FLOOR_DB);
    (1.0 - f64::from(byte) / 255.0) * floor
}

/// The lowest and highest peak between `from_ms` and `to_ms`, in dBFS,
/// to a tenth; none when the range holds no point.
pub fn levels(peaks: &[u8], interval_ms: u32, from_ms: u32, to_ms: u32) -> Option<(f64, f64)> {
    let interval = interval_ms.max(1);
    let first = (from_ms / interval) as usize;
    let last = (to_ms.div_ceil(interval) as usize).min(peaks.len());
    let range = peaks.get(first..last).filter(|range| !range.is_empty())?;
    let round = |db: f64| (db * 10.0).round() / 10.0;
    let min = range.iter().copied().min()?;
    let max = range.iter().copied().max()?;
    Some((round(byte_db(min)), round(byte_db(max))))
}

/// The cue points the levels are computed between: the overrides first,
/// the whole file when Gap Killer is off for the track.
const CUES: &str = "COALESCE(tov.cue_in_ms, IF(tov.gap_killer_enabled = FALSE, 0, a.cue_in_ms))";
const CUE_OUT: &str =
    "COALESCE(tov.cue_out_ms, IF(tov.gap_killer_enabled = FALSE, NULL, a.cue_out_ms), 4294967295)";

/// Computes the levels of the tracks whose waveform or cue points changed;
/// how many.
pub async fn update(pool: &MySqlPool) -> ApiResult<usize> {
    let sql = format!(
        "SELECT id, interval_ms, peaks, analyzed_at, cue_in, cue_out FROM (
            SELECT t.id, w.interval_ms, w.peaks, CAST(a.analyzed_at AS CHAR) AS analyzed_at,
                   CAST({CUES} AS UNSIGNED) AS cue_in, CAST({CUE_OUT} AS UNSIGNED) AS cue_out,
                   l.track_id AS done, CAST(l.analyzed_at AS CHAR) AS done_at,
                   l.cue_in_ms AS done_in, l.cue_out_ms AS done_out
            FROM tracks t
            JOIN track_analysis a ON a.track_id = t.id
            JOIN track_waveforms w ON w.track_id = t.id
            LEFT JOIN track_overrides tov ON tov.track_id = t.id
            LEFT JOIN track_levels l ON l.track_id = t.id
         ) x
         WHERE done IS NULL OR done_at <> analyzed_at OR done_in <> cue_in OR done_out <> cue_out
         LIMIT {BATCH}"
    );
    let rows: Vec<(u64, u16, Vec<u8>, String, u64, u64)> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .fetch_all(pool)
        .await?;
    let count = rows.len();
    for (id, interval_ms, peaks, analyzed_at, cue_in, cue_out) in rows {
        let cue_in = u32::try_from(cue_in).unwrap_or(END);
        let cue_out = u32::try_from(cue_out).unwrap_or(END);
        // A range without a point (a silent track) reads as the floor.
        let floor = f64::from(WAVEFORM_FLOOR_DB);
        let (min_db, max_db) =
            levels(&peaks, u32::from(interval_ms), cue_in, cue_out).unwrap_or((floor, floor));
        sqlx::query(
            "INSERT INTO track_levels (track_id, min_db, max_db, cue_in_ms, cue_out_ms, analyzed_at)
             VALUES (?, ?, ?, ?, ?, ?)
             ON DUPLICATE KEY UPDATE min_db = VALUES(min_db), max_db = VALUES(max_db),
                 cue_in_ms = VALUES(cue_in_ms), cue_out_ms = VALUES(cue_out_ms),
                 analyzed_at = VALUES(analyzed_at)",
        )
        .bind(id)
        .bind(min_db)
        .bind(max_db)
        .bind(cue_in)
        .bind(cue_out)
        .bind(analyzed_at)
        .execute(pool)
        .await?;
    }
    Ok(count)
}

/// Keeps the levels up to date, a batch every few seconds.
pub async fn run(pool: MySqlPool) {
    loop {
        match update(&pool).await {
            Ok(0) => tokio::time::sleep(INTERVAL).await,
            Ok(_) => {}
            Err(error) => {
                tracing::warn!(%error, "cannot compute the tracks' levels");
                tokio::time::sleep(INTERVAL).await;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bytes_are_dbfs() {
        assert!((byte_db(255) - 0.0).abs() < 1e-9);
        assert!((byte_db(0) + 72.0).abs() < 1e-9);
        assert!((byte_db(128) + 35.86).abs() < 0.01);
    }

    #[test]
    fn levels_are_taken_between_the_cue_points() {
        // 50 ms points: silence, the music, silence.
        let peaks = [0, 0, 200, 255, 150, 220, 0, 0];
        assert_eq!(levels(&peaks, 50, 0, END), Some((-72.0, 0.0)));
        // From 100 ms to 300 ms: the music only.
        let quietest = (byte_db(150) * 10.0).round() / 10.0;
        assert_eq!(levels(&peaks, 50, 100, 300), Some((quietest, 0.0)));
        // A cue point inside a point takes that point.
        assert_eq!(levels(&peaks, 50, 120, 280).map(|(_, max)| max), Some(0.0));
        assert_eq!(levels(&peaks, 50, 400, 500), None);
        assert_eq!(levels(&[], 50, 0, END), None);
    }
}
