// SPDX-License-Identifier: AGPL-3.0-or-later

//! Worker loop: claim a job, analyze the file, store the result.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use heron_analysis::jobs::{self, Completion, Job};
use heron_core::FileStamp;
use sqlx::MySqlPool;
use tokio::sync::watch;
use tokio::task::JoinSet;

/// Wait between polls when the queue is empty.
const IDLE_POLL: Duration = Duration::from_secs(2);
/// A running job is reclaimed after this long (the worker probably died).
const LEASE: Duration = Duration::from_secs(300);

pub struct WorkerConfig {
    library_dir: PathBuf,
    instance: String,
}

impl WorkerConfig {
    pub fn new(library_dir: PathBuf, instance: String) -> Self {
        Self {
            library_dir,
            instance,
        }
    }
}

/// Starts `count` workers. The returned future resolves once they all
/// stopped, after `stopped` turns true.
pub fn spawn(
    pool: MySqlPool,
    config: WorkerConfig,
    count: usize,
    stopped: watch::Receiver<bool>,
) -> impl Future<Output = ()> {
    let config = Arc::new(config);
    let mut workers = JoinSet::new();
    for index in 1..=count {
        let worker = format!("{}/{index}", config.instance);
        workers.spawn(run(pool.clone(), config.clone(), worker, stopped.clone()));
    }
    async move { while workers.join_next().await.is_some() {} }
}

async fn run(
    pool: MySqlPool,
    config: Arc<WorkerConfig>,
    worker: String,
    mut stopped: watch::Receiver<bool>,
) {
    while !*stopped.borrow() {
        let idle = match jobs::claim(&pool, &worker, LEASE).await {
            Ok(Some(job)) => {
                process(&pool, &config, job).await;
                false
            }
            Ok(None) => true,
            Err(error) => {
                tracing::warn!(%worker, %error, "cannot claim a job");
                true
            }
        };
        if idle {
            tokio::select! {
                () = tokio::time::sleep(IDLE_POLL) => {}
                _ = stopped.changed() => {}
            }
        }
    }
}

async fn process(pool: &MySqlPool, config: &WorkerConfig, job: Job) {
    let path = job.path.resolve(&config.library_dir);
    let result = analyze(pool, path, &job).await;
    let outcome = match result {
        Ok(Completion::Saved) => "done",
        Ok(Completion::FileChanged) => "file_changed",
        Ok(Completion::TrackGone) => "track_gone",
        Err(error) => {
            let retry = error.is_transient();
            match jobs::retry_or_fail(pool, &job, &error.to_string(), retry).await {
                Ok(true) => {
                    tracing::warn!(path = %job.path, attempt = job.attempts, %error, "analysis failed, will retry");
                    "retry"
                }
                Ok(false) => {
                    tracing::warn!(path = %job.path, %error, "analysis failed");
                    "failed"
                }
                Err(db_error) => {
                    // The lease expires and another worker picks the job up.
                    tracing::error!(path = %job.path, %db_error, "cannot record the failure");
                    "error"
                }
            }
        }
    };
    metrics::counter!("heron_analysis_jobs_total", "result" => outcome).increment(1);
}

async fn analyze(
    pool: &MySqlPool,
    path: PathBuf,
    job: &Job,
) -> Result<Completion, heron_analysis::Error> {
    let settings = jobs::load_settings(pool).await?;
    let stamp = FileStamp::from_metadata(&tokio::fs::metadata(&path).await?);
    let started = Instant::now();
    let task_settings = settings.clone();
    let analysis =
        tokio::task::spawn_blocking(move || heron_analysis::analyze_file(&path, &task_settings))
            .await
            .map_err(|error| std::io::Error::other(format!("analysis task failed: {error}")))??;
    let elapsed = started.elapsed();

    let completion = jobs::complete(pool, job, stamp, &analysis, &settings).await?;
    metrics::histogram!("heron_analysis_seconds").record(elapsed.as_secs_f64());
    metrics::counter!("heron_analysis_audio_ms_total").increment(u64::from(analysis.duration_ms));
    tracing::debug!(
        path = %job.path,
        elapsed_ms = elapsed.as_millis() as u64,
        cue_in_ms = analysis.cue_in_ms,
        cue_out_ms = analysis.cue_out_ms,
        lufs = analysis.integrated_lufs,
        "analyzed"
    );
    if analysis.is_silent() {
        tracing::warn!(path = %job.path, "track is silent, it will not be played");
    }
    Ok(completion)
}
