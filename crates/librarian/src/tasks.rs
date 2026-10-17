// SPDX-License-Identifier: AGPL-3.0-or-later

//! Work asked through the API (`library_tasks`), which cannot write files:
//! organize dry runs and applies, undos, releases picked on the review
//! screen, and MusicBrainz relationships between artists. Tasks run one at a time, in order; their results are stored as
//! JSON for the API to show.

use std::time::Duration;

use heron_core::library::{OrganizePlan, TaskRequest};
use sqlx::MySqlPool;
use tokio::sync::watch;

use crate::identify::{Endpoints, Identifier};
use crate::journal::Roots;
use crate::layout::Layout;
use crate::review::{self, Choice};
use crate::{import, organize, relations};

/// Wait between looks at the table when there is nothing to do.
const POLL: Duration = Duration::from_secs(1);

pub struct TaskRunner {
    pool: MySqlPool,
    roots: Roots,
    endpoints: Endpoints,
}

impl TaskRunner {
    pub fn new(pool: MySqlPool, roots: Roots, endpoints: Endpoints) -> Self {
        Self {
            pool,
            roots,
            endpoints,
        }
    }

    /// Runs tasks until `stopped` turns true. A task that is running then
    /// finishes first.
    pub async fn run(self, mut stopped: watch::Receiver<bool>) {
        // A task running when the service stopped was cut short: undos are
        // resumed by asking again, a partial organization is undone or
        // finished like one interrupted on the command line.
        if let Err(error) = sqlx::query(
            "UPDATE library_tasks SET status = 'failed', finished_at = NOW(6),
                 error = 'interrupted: heron-librarian stopped while it ran'
             WHERE status = 'running'",
        )
        .execute(&self.pool)
        .await
        {
            tracing::warn!(%error, "cannot close interrupted library tasks");
        }
        while !*stopped.borrow() {
            match self.run_next().await {
                Ok(true) => continue,
                Ok(false) => {}
                Err(error) => tracing::warn!(%error, "cannot read library tasks"),
            }
            tokio::select! {
                () = tokio::time::sleep(POLL) => {}
                _ = stopped.changed() => {}
            }
        }
    }

    /// Runs the oldest pending task. Returns whether there was one.
    pub async fn run_next(&self) -> Result<bool, sqlx::Error> {
        let next: Option<(u64, String)> = sqlx::query_as(
            "SELECT id, CAST(params AS CHAR) FROM library_tasks
             WHERE status = 'pending' ORDER BY id LIMIT 1",
        )
        .fetch_optional(&self.pool)
        .await?;
        let Some((id, params)) = next else {
            return Ok(false);
        };
        let claimed = sqlx::query(
            "UPDATE library_tasks SET status = 'running', started_at = NOW(6)
             WHERE id = ? AND status = 'pending'",
        )
        .bind(id)
        .execute(&self.pool)
        .await?
        .rows_affected();
        if claimed == 0 {
            return Ok(true);
        }
        let outcome = match serde_json::from_str::<TaskRequest>(&params) {
            Ok(request) => {
                tracing::info!(id, kind = request.kind(), "running a library task");
                self.execute(id, &request).await
            }
            Err(error) => Err(format!("invalid task: {error}")),
        };
        match &outcome {
            Ok(_) => tracing::info!(id, "library task done"),
            Err(error) => tracing::warn!(id, error, "library task failed"),
        }
        let (status, result, error) = match outcome {
            Ok(result) => ("done", Some(result.to_string()), None),
            Err(error) => ("failed", None, Some(error)),
        };
        sqlx::query(
            "UPDATE library_tasks SET status = ?, result = ?, error = ?, finished_at = NOW(6)
             WHERE id = ?",
        )
        .bind(status)
        .bind(result)
        .bind(error)
        .bind(id)
        .execute(&self.pool)
        .await?;
        Ok(true)
    }

    async fn execute(&self, id: u64, request: &TaskRequest) -> Result<serde_json::Value, String> {
        let settings = import::load_settings(&self.pool)
            .await
            .map_err(|error| error.to_string())?;
        match request {
            TaskRequest::OrganizePlan => {
                let layout = Layout::from_settings(&settings);
                json(organize::plan(&self.pool, &self.roots, &layout).await)
            }
            TaskRequest::OrganizeApply { plan_task_id } => {
                let plan = self.plan(id, *plan_task_id).await?;
                json(organize::apply(&self.pool, &self.roots, &plan).await)
            }
            TaskRequest::Undo { batch_id } => {
                json(organize::undo(&self.pool, &self.roots, *batch_id).await)
            }
            TaskRequest::ReviewApply {
                track_id,
                release_id,
                recording_id,
                disc,
                track,
            } => {
                // Looking a release up needs no AcoustID key.
                let key = settings.acoustid_key.clone().unwrap_or_default();
                let identifier = Identifier::new(self.endpoints.clone(), key, settings.min_score)
                    .map_err(|error| error.to_string())?;
                let choice = Choice {
                    track_id: *track_id,
                    release_id: release_id.clone(),
                    recording_id: recording_id.clone(),
                    disc: *disc,
                    track: *track,
                };
                json(review::apply(&self.pool, &self.roots, &identifier, &settings, &choice).await)
            }
            TaskRequest::ArtistRelations => {
                // MusicBrainz needs no AcoustID key.
                let identifier = Identifier::new(self.endpoints.clone(), String::new(), 1.0)
                    .map_err(|error| error.to_string())?;
                json(relations::look_up(&self.pool, &identifier).await)
            }
        }
    }

    /// The plan of a finished dry run, not applied by another task.
    async fn plan(&self, id: u64, plan_task_id: u64) -> Result<OrganizePlan, String> {
        let row: Option<(String, String, Option<String>)> = sqlx::query_as(
            "SELECT CAST(kind AS CHAR), CAST(status AS CHAR), CAST(result AS CHAR)
             FROM library_tasks WHERE id = ?",
        )
        .bind(plan_task_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| error.to_string())?;
        let Some((_, _, Some(result))) =
            row.filter(|(kind, status, _)| kind == "organize_plan" && status == "done")
        else {
            return Err(format!("task {plan_task_id} is not a finished dry run"));
        };
        // A failed apply can be run again: moves done since are skipped.
        let applied: Option<u64> = sqlx::query_scalar(
            "SELECT id FROM library_tasks
             WHERE kind = 'organize_apply' AND status = 'done' AND id <> ?
               AND JSON_VALUE(params, '$.plan_task_id') = ?
             LIMIT 1",
        )
        .bind(id)
        .bind(plan_task_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| error.to_string())?;
        if let Some(applied) = applied {
            return Err(format!("this dry run was applied already (task {applied})"));
        }
        serde_json::from_str(&result).map_err(|error| format!("invalid dry run: {error}"))
    }
}

/// A task's result as stored, or its error for people.
fn json<T: serde::Serialize, E: std::fmt::Display>(
    result: Result<T, E>,
) -> Result<serde_json::Value, String> {
    let value = result.map_err(|error| error.to_string())?;
    serde_json::to_value(value).map_err(|error| error.to_string())
}
