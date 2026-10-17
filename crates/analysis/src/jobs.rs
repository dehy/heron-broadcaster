// SPDX-License-Identifier: AGPL-3.0-or-later

//! The analysis job queue in MariaDB (`analysis_jobs`).
//!
//! The scanner in `heron-api` enqueues tracks whose analysis is missing or
//! out of date; `heron-analyzer` workers claim jobs with
//! `SELECT ... FOR UPDATE SKIP LOCKED`, so any number of workers and
//! replicas can share the queue. See `docs/architecture.md`.

use std::time::Duration;

use heron_core::{AnalysisSettings, FileStamp, LibraryPath};
use sqlx::{MySql, MySqlPool, Row};

use crate::{ALGORITHM_VERSION, Analysis, Chromaprint, Error};

/// A job is failed for good after this many attempts.
pub const MAX_ATTEMPTS: u32 = 3;

/// Longest error message stored with a job.
const MAX_ERROR_LEN: usize = 1_000;

/// A claimed job.
#[derive(Clone, Debug)]
pub struct Job {
    pub id: u64,
    pub track_id: u64,
    /// Including the current attempt.
    pub attempts: u32,
    pub path: LibraryPath,
    /// File stamp recorded by the scanner. The analysis is only saved if the
    /// file still has this stamp, so a file replaced mid-analysis is redone.
    pub stamp: Option<FileStamp>,
}

/// Number of jobs per status.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct QueueCounts {
    pub pending: u64,
    pub running: u64,
    pub done: u64,
    pub failed: u64,
}

/// Delay before a job whose file changed is tried again.
pub const FILE_CHANGED_DELAY: Duration = Duration::from_secs(30);

/// What [`complete`] did with a result.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Completion {
    Saved,
    /// The file is not the one the scanner recorded (still being copied, or
    /// the scanner has not seen the change yet): the job is postponed.
    FileChanged,
    /// The track was deleted meanwhile: the result is dropped.
    TrackGone,
}

/// Reads the analysis settings from the `settings` table, falling back to
/// the defaults when the row is missing or invalid.
pub async fn load_settings(pool: &MySqlPool) -> Result<AnalysisSettings, Error> {
    let value: Option<String> =
        sqlx::query_scalar("SELECT CAST(value AS CHAR) FROM settings WHERE name = ?")
            .bind(AnalysisSettings::SETTINGS_NAME)
            .fetch_optional(pool)
            .await?;
    let Some(value) = value else {
        return Ok(AnalysisSettings::default());
    };
    match serde_json::from_str(&value) {
        Ok(settings) => Ok(settings),
        Err(error) => {
            tracing::error!(%error, "invalid analysis settings, using the defaults");
            Ok(AnalysisSettings::default())
        }
    }
}

/// Queues every present track whose analysis is missing, made by another
/// algorithm version or made with other settings. Tracks with a pending or
/// running job are skipped, and so are tracks whose last job failed for good
/// unless the file changed since. Returns the number of jobs added.
pub async fn enqueue_stale(pool: &MySqlPool, settings: &AnalysisSettings) -> Result<u64, Error> {
    let result = sqlx::query(
        "INSERT INTO analysis_jobs (track_id, analysis_version)
         SELECT t.id, ? FROM tracks t
         LEFT JOIN track_analysis a ON a.track_id = t.id
         WHERE t.missing_since IS NULL
           AND (a.track_id IS NULL OR a.analysis_version <> ? OR a.params_hash <> ?)
           AND NOT EXISTS (
               SELECT 1 FROM analysis_jobs j
               WHERE j.track_id = t.id
                 AND (j.status IN ('pending', 'running')
                      OR (j.status = 'failed' AND j.analysis_version = ?
                          AND j.updated_at >= t.updated_at)))",
    )
    .bind(ALGORITHM_VERSION)
    .bind(ALGORITHM_VERSION)
    .bind(settings.fingerprint().as_slice())
    .bind(ALGORITHM_VERSION)
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}

/// Claims the oldest pending job that is due, or a running one whose worker
/// has held it for longer than `lease` (the worker probably died).
pub async fn claim(pool: &MySqlPool, worker: &str, lease: Duration) -> Result<Option<Job>, Error> {
    let mut tx = pool.begin().await?;
    let row = sqlx::query(
        "SELECT id, track_id, attempts FROM analysis_jobs
         WHERE (status = 'pending' AND (not_before IS NULL OR not_before <= NOW(6)))
            OR (status = 'running' AND claimed_at < NOW(6) - INTERVAL ? SECOND)
         ORDER BY id
         LIMIT 1
         FOR UPDATE SKIP LOCKED",
    )
    .bind(lease.as_secs())
    .fetch_optional(&mut *tx)
    .await?;
    let Some(row) = row else {
        tx.commit().await?;
        return Ok(None);
    };
    let id: u64 = row.try_get("id")?;
    let track_id: u64 = row.try_get("track_id")?;
    let attempts: u32 = row.try_get::<u32, _>("attempts")? + 1;

    sqlx::query(
        "UPDATE analysis_jobs
         SET status = 'running', claimed_by = ?, claimed_at = NOW(6), attempts = ?
         WHERE id = ?",
    )
    .bind(worker)
    .bind(attempts)
    .bind(id)
    .execute(&mut *tx)
    .await?;

    let track = sqlx::query("SELECT path, file_size, file_mtime_us FROM tracks WHERE id = ?")
        .bind(track_id)
        .fetch_optional(&mut *tx)
        .await?;
    let Some(track) = track else {
        // The track was deleted: the job cascades away with it, nothing to do.
        tx.commit().await?;
        return Ok(None);
    };
    tx.commit().await?;

    Ok(Some(Job {
        id,
        track_id,
        attempts,
        path: LibraryPath::from_stored(track.try_get("path")?),
        stamp: stamp_from_row(&track)?,
    }))
}

fn stamp_from_row(row: &sqlx::mysql::MySqlRow) -> Result<Option<FileStamp>, Error> {
    let size: Option<u64> = row.try_get("file_size")?;
    let mtime_us: Option<i64> = row.try_get("file_mtime_us")?;
    Ok(size
        .zip(mtime_us)
        .map(|(size, mtime_us)| FileStamp { size, mtime_us }))
}

/// Saves the analysis and fingerprint of `job`, made from a file with stamp
/// `analyzed`.
pub async fn complete(
    pool: &MySqlPool,
    job: &Job,
    analyzed: FileStamp,
    analysis: &Analysis,
    chromaprint: Option<&Chromaprint>,
    settings: &AnalysisSettings,
) -> Result<Completion, Error> {
    let mut tx = pool.begin().await?;
    let track = sqlx::query("SELECT file_size, file_mtime_us FROM tracks WHERE id = ? FOR UPDATE")
        .bind(job.track_id)
        .fetch_optional(&mut *tx)
        .await?;
    let Some(track) = track else {
        tx.commit().await?;
        return Ok(Completion::TrackGone);
    };
    if stamp_from_row(&track)? != Some(analyzed) {
        tx.rollback().await?;
        postpone(
            pool,
            job,
            "the file differs from the scanned one",
            FILE_CHANGED_DELAY,
        )
        .await?;
        return Ok(Completion::FileChanged);
    }

    sqlx::query("UPDATE tracks SET duration_ms = ? WHERE id = ?")
        .bind(analysis.duration_ms)
        .bind(job.track_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query(
        "INSERT INTO track_analysis (
             track_id, analysis_version, params_hash, integrated_lufs, true_peak_db,
             cue_in_ms, cue_out_ms, fade_out_start_ms, has_internal_silence, analyzed_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, NOW(6))
         ON DUPLICATE KEY UPDATE
             analysis_version = VALUES(analysis_version),
             params_hash = VALUES(params_hash),
             integrated_lufs = VALUES(integrated_lufs),
             true_peak_db = VALUES(true_peak_db),
             cue_in_ms = VALUES(cue_in_ms),
             cue_out_ms = VALUES(cue_out_ms),
             fade_out_start_ms = VALUES(fade_out_start_ms),
             has_internal_silence = VALUES(has_internal_silence),
             analyzed_at = VALUES(analyzed_at)",
    )
    .bind(job.track_id)
    .bind(ALGORITHM_VERSION)
    .bind(settings.fingerprint().as_slice())
    .bind(analysis.integrated_lufs)
    .bind(analysis.true_peak_db)
    .bind(analysis.cue_in_ms)
    .bind(analysis.cue_out_ms)
    .bind(analysis.fade_out_start_ms)
    .bind(analysis.has_internal_silence)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO track_waveforms (track_id, interval_ms, peaks) VALUES (?, ?, ?)
         ON DUPLICATE KEY UPDATE interval_ms = VALUES(interval_ms), peaks = VALUES(peaks)",
    )
    .bind(job.track_id)
    .bind(analysis.waveform.interval_ms)
    .bind(analysis.waveform.peaks.as_slice())
    .execute(&mut *tx)
    .await?;
    if let Some(chromaprint) = chromaprint {
        sqlx::query(
            "INSERT INTO track_fingerprints (track_id, chromaprint) VALUES (?, ?)
             ON DUPLICATE KEY UPDATE chromaprint = VALUES(chromaprint), created_at = NOW(6)",
        )
        .bind(job.track_id)
        .bind(chromaprint.as_bytes())
        .execute(&mut *tx)
        .await?;
    }
    sqlx::query("UPDATE analysis_jobs SET status = 'done', error = NULL WHERE id = ?")
        .bind(job.id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(Completion::Saved)
}

/// Delay before retry number `attempt` (1-based) of a failed job.
pub fn retry_delay(attempt: u32) -> Duration {
    Duration::from_secs(30).saturating_mul(1 << attempt.saturating_sub(1).min(6))
}

/// Records a failed attempt. The job goes back to the queue after `delay`
/// when `retry` is set and attempts remain, and is failed for good
/// otherwise. Returns whether it will be retried.
pub async fn retry_or_fail(
    pool: &MySqlPool,
    job: &Job,
    error: &str,
    retry: bool,
    delay: Duration,
) -> Result<bool, Error> {
    let retry = retry && job.attempts < MAX_ATTEMPTS;
    let status = if retry { "pending" } else { "failed" };
    let mut tx = pool.begin().await?;
    sqlx::query(
        "UPDATE analysis_jobs
         SET status = ?, error = ?, claimed_by = NULL, claimed_at = NULL,
             not_before = NOW(6) + INTERVAL ? MICROSECOND
         WHERE id = ?",
    )
    .bind(status)
    .bind(truncate(error))
    .bind(micros(delay))
    .bind(job.id)
    .execute(&mut *tx)
    .await?;
    if !retry {
        // An older analysis of a file that no longer decodes must not be used.
        sqlx::query("DELETE FROM track_analysis WHERE track_id = ?")
            .bind(job.track_id)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(retry)
}

/// Puts a job back for later without counting the attempt: the file was not
/// ready, which says nothing about whether it can be analyzed.
pub async fn postpone(
    pool: &MySqlPool,
    job: &Job,
    reason: &str,
    delay: Duration,
) -> Result<(), Error> {
    sqlx::query(
        "UPDATE analysis_jobs
         SET status = 'pending', error = ?, claimed_by = NULL, claimed_at = NULL,
             attempts = GREATEST(attempts, 1) - 1,
             not_before = NOW(6) + INTERVAL ? MICROSECOND
         WHERE id = ?",
    )
    .bind(truncate(reason))
    .bind(micros(delay))
    .bind(job.id)
    .execute(pool)
    .await?;
    Ok(())
}

fn truncate(error: &str) -> String {
    error.chars().take(MAX_ERROR_LEN).collect()
}

fn micros(delay: Duration) -> u64 {
    u64::try_from(delay.as_micros()).unwrap_or(u64::MAX)
}

/// Puts back the running jobs of a worker instance (on shutdown, or at
/// startup after a crash) without counting the interrupted attempt.
pub async fn release(pool: &MySqlPool, instance: &str) -> Result<u64, Error> {
    let result = sqlx::query(
        "UPDATE analysis_jobs
         SET status = 'pending', claimed_by = NULL, claimed_at = NULL,
             attempts = GREATEST(attempts, 1) - 1
         WHERE status = 'running' AND claimed_by LIKE CONCAT(?, '/%')",
    )
    .bind(instance)
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}

/// Number of jobs per status.
pub async fn counts(pool: &MySqlPool) -> Result<QueueCounts, Error> {
    let rows = sqlx::query::<MySql>(
        "SELECT CAST(status AS CHAR) AS status, COUNT(*) AS jobs
         FROM analysis_jobs GROUP BY status",
    )
    .fetch_all(pool)
    .await?;
    let mut counts = QueueCounts::default();
    for row in rows {
        let jobs = u64::try_from(row.try_get::<i64, _>("jobs")?).unwrap_or(0);
        match row.try_get::<String, _>("status")?.as_str() {
            "pending" => counts.pending = jobs,
            "running" => counts.running = jobs,
            "done" => counts.done = jobs,
            "failed" => counts.failed = jobs,
            _ => {}
        }
    }
    Ok(counts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use heron_service::db::ScratchDatabase;

    const LEASE: Duration = Duration::from_secs(300);
    const STAMP: FileStamp = FileStamp {
        size: 1_000,
        mtime_us: 1_700_000_000_000_000,
    };

    async fn add_track(pool: &MySqlPool, path: &str) -> u64 {
        sqlx::query("INSERT INTO tracks (path, file_size, file_mtime_us) VALUES (?, ?, ?)")
            .bind(path)
            .bind(STAMP.size)
            .bind(STAMP.mtime_us)
            .execute(pool)
            .await
            .unwrap()
            .last_insert_id()
    }

    fn chromaprint() -> Chromaprint {
        Chromaprint::from_compressed(vec![1, 0, 0, 2, 42])
    }

    fn analysis() -> Analysis {
        Analysis {
            duration_ms: 180_000,
            integrated_lufs: Some(-11.5),
            true_peak_db: Some(-0.4),
            cue_in_ms: 120,
            cue_out_ms: 178_500,
            fade_out_start_ms: Some(176_000),
            has_internal_silence: false,
            waveform: crate::Waveform {
                interval_ms: 50,
                peaks: vec![0, 128, 255],
            },
        }
    }

    #[tokio::test]
    async fn analyzed_tracks_are_not_queued_again_until_settings_change() {
        let Some(db) = ScratchDatabase::create().await else {
            return;
        };
        let pool = &db.pool;
        let settings = AnalysisSettings::default();
        let track_id = add_track(pool, "a.flac").await;
        add_track(pool, "b.flac").await;

        assert_eq!(enqueue_stale(pool, &settings).await.unwrap(), 2);
        assert_eq!(
            enqueue_stale(pool, &settings).await.unwrap(),
            0,
            "already queued"
        );

        let job = claim(pool, "test/1", LEASE).await.unwrap().unwrap();
        assert_eq!(job.track_id, track_id);
        assert_eq!(job.path.as_str(), "a.flac");
        assert_eq!(job.stamp, Some(STAMP));
        let completion = complete(
            pool,
            &job,
            STAMP,
            &analysis(),
            Some(&chromaprint()),
            &settings,
        )
        .await
        .unwrap();
        assert_eq!(completion, Completion::Saved);

        let saved: (u32, u32) =
            sqlx::query_as("SELECT cue_in_ms, cue_out_ms FROM track_analysis WHERE track_id = ?")
                .bind(track_id)
                .fetch_one(pool)
                .await
                .unwrap();
        assert_eq!(saved, (120, 178_500));
        let stored: Vec<u8> =
            sqlx::query_scalar("SELECT chromaprint FROM track_fingerprints WHERE track_id = ?")
                .bind(track_id)
                .fetch_one(pool)
                .await
                .unwrap();
        assert_eq!(stored, chromaprint().as_bytes());
        let waveform: (u16, Vec<u8>) =
            sqlx::query_as("SELECT interval_ms, peaks FROM track_waveforms WHERE track_id = ?")
                .bind(track_id)
                .fetch_one(pool)
                .await
                .unwrap();
        assert_eq!(waveform, (50, vec![0, 128, 255]));
        assert_eq!(enqueue_stale(pool, &settings).await.unwrap(), 0);

        let changed = AnalysisSettings {
            end_threshold_db: -30.0,
            ..AnalysisSettings::default()
        };
        assert_eq!(
            enqueue_stale(pool, &changed).await.unwrap(),
            1,
            "a.flac is re-queued, b.flac is still pending"
        );
        db.drop().await;
    }

    #[tokio::test]
    async fn results_for_a_replaced_file_are_not_saved() {
        let Some(db) = ScratchDatabase::create().await else {
            return;
        };
        let pool = &db.pool;
        let settings = AnalysisSettings::default();
        add_track(pool, "a.flac").await;
        enqueue_stale(pool, &settings).await.unwrap();

        let job = claim(pool, "test/1", LEASE).await.unwrap().unwrap();
        let other = FileStamp {
            size: 2_000,
            ..STAMP
        };
        let completion = complete(pool, &job, other, &analysis(), None, &settings)
            .await
            .unwrap();
        assert_eq!(completion, Completion::FileChanged);
        assert_eq!(counts(pool).await.unwrap().pending, 1, "queued again");
        assert!(
            claim(pool, "test/1", LEASE).await.unwrap().is_none(),
            "postponed, not claimable at once"
        );
        sqlx::query("UPDATE analysis_jobs SET not_before = NULL")
            .execute(pool)
            .await
            .unwrap();
        let again = claim(pool, "test/1", LEASE).await.unwrap().unwrap();
        assert_eq!(again.attempts, 1, "a postponed attempt is not counted");
        db.drop().await;
    }

    #[tokio::test]
    async fn failed_jobs_are_retried_then_given_up() {
        let Some(db) = ScratchDatabase::create().await else {
            return;
        };
        let pool = &db.pool;
        let settings = AnalysisSettings::default();
        let track_id = add_track(pool, "broken.mp3").await;
        // An analysis from an older version, which re-analysis will replace.
        sqlx::query(
            "INSERT INTO track_analysis (track_id, analysis_version, params_hash, cue_in_ms, cue_out_ms)
             VALUES (?, 0, '', 0, 1000)",
        )
        .bind(track_id)
        .execute(pool)
        .await
        .unwrap();
        enqueue_stale(pool, &settings).await.unwrap();

        for attempt in 1..=MAX_ATTEMPTS {
            let job = claim(pool, "test/1", LEASE).await.unwrap().unwrap();
            assert_eq!(job.attempts, attempt);
            let retried = retry_or_fail(pool, &job, "disk hiccup", true, Duration::ZERO)
                .await
                .unwrap();
            assert_eq!(retried, attempt < MAX_ATTEMPTS);
        }
        assert!(claim(pool, "test/1", LEASE).await.unwrap().is_none());
        assert_eq!(counts(pool).await.unwrap().failed, 1);
        let analyses: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM track_analysis")
            .fetch_one(pool)
            .await
            .unwrap();
        assert_eq!(analyses, 0, "no stale analysis left behind");
        assert_eq!(
            enqueue_stale(pool, &settings).await.unwrap(),
            0,
            "not re-queued while the file is unchanged"
        );
        db.drop().await;
    }

    #[test]
    fn retry_delays_grow_and_are_capped() {
        assert_eq!(retry_delay(1), Duration::from_secs(30));
        assert_eq!(retry_delay(2), Duration::from_secs(60));
        assert_eq!(retry_delay(50), Duration::from_secs(30 * 64));
    }

    #[tokio::test]
    async fn jobs_of_a_dead_worker_are_reclaimed() {
        let Some(db) = ScratchDatabase::create().await else {
            return;
        };
        let pool = &db.pool;
        let settings = AnalysisSettings::default();
        add_track(pool, "a.flac").await;
        enqueue_stale(pool, &settings).await.unwrap();

        claim(pool, "old/1", LEASE).await.unwrap().unwrap();
        assert!(
            claim(pool, "new/1", LEASE).await.unwrap().is_none(),
            "lease held"
        );
        let expired = claim(pool, "new/1", Duration::ZERO).await.unwrap();
        assert!(expired.is_some(), "an expired lease is reclaimed");

        assert_eq!(release(pool, "new").await.unwrap(), 1);
        let job = claim(pool, "new/1", LEASE).await.unwrap().unwrap();
        assert_eq!(job.attempts, 2, "a released attempt is not counted");
        db.drop().await;
    }
}
