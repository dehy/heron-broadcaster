// SPDX-License-Identifier: AGPL-3.0-or-later

//! Applying a release picked on the review screen to a track whose
//! identification was not confident enough: the release's tags are written
//! into the file and it is filed where the layout says, as one batch that
//! can be undone like any other.

use std::collections::HashSet;

use heron_core::library::{BatchReport, SkippedFile};
use heron_core::{LibraryPath, LibrarySettings};
use heron_library::tags;
use heron_library::{LibraryLock, Tags};

use crate::identify::{Identifier, IdentifyError};
use crate::import;
use crate::journal::{
    self, BatchKind, OpError, Operation, Origin, Retag, RetagCover, Roots, Subject,
};
use crate::layout::{FileFacts, Layout};
use crate::organize;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error(transparent)]
    Operation(#[from] OpError),
    #[error(transparent)]
    Identify(#[from] IdentifyError),
    #[error("{0}")]
    Refused(String),
    #[error("task failed: {0}")]
    Task(#[from] tokio::task::JoinError),
}

/// The release picked for a track, among its candidates.
#[derive(Clone, Debug)]
pub struct Choice {
    pub track_id: u64,
    pub release_id: String,
    pub recording_id: String,
    pub disc: Option<u32>,
    pub track: Option<u32>,
}

/// Applies `choice`. Takes the library lock for the file operation.
pub async fn apply(
    pool: &sqlx::MySqlPool,
    roots: &Roots,
    identifier: &Identifier,
    settings: &LibrarySettings,
    choice: &Choice,
) -> Result<BatchReport, Error> {
    let row: Option<(String, bool, Option<String>, Option<f64>)> = sqlx::query_as(
        "SELECT t.path, t.missing_since IS NOT NULL, CAST(i.status AS CHAR), i.score
         FROM tracks t LEFT JOIN track_identifications i ON i.track_id = t.id
         WHERE t.id = ?",
    )
    .bind(choice.track_id)
    .fetch_optional(pool)
    .await?;
    let Some((path, missing, status, score)) = row else {
        return Err(Error::Refused(format!("no track {}", choice.track_id)));
    };
    if missing {
        return Err(Error::Refused("the track's file is missing".into()));
    }
    if status.as_deref() != Some("needs_review") {
        return Err(Error::Refused("the track does not need review".into()));
    }

    let Some(identified) = identifier
        .release_tags(
            &choice.release_id,
            &choice.recording_id,
            choice.disc,
            choice.track,
        )
        .await?
    else {
        return Err(Error::Refused(
            "MusicBrainz does not list this recording on the release".into(),
        ));
    };
    let cover = if settings.save_cover || settings.embed_cover {
        identifier
            .cover(&choice.release_id)
            .await
            .unwrap_or_else(|error| {
                tracing::warn!(release = choice.release_id, %error, "cannot fetch the cover");
                None
            })
    } else {
        None
    };

    let current = LibraryPath::from_stored(path);
    let absolute = current.resolve(&roots.library);
    let read = {
        let absolute = absolute.clone();
        tokio::task::spawn_blocking(move || tags::read(&absolute))
            .await?
            .map_err(OpError::from)?
    };
    let own: Tags = read.tags;
    let applied = import::merge(identified, &own);
    let embed = cover
        .as_ref()
        .filter(|_| settings.embed_cover && !read.has_cover);
    let retag = (applied != own || embed.is_some()).then(|| Retag {
        previous: own.clone(),
        applied: applied.clone(),
        cover: embed.map_or(RetagCover::Keep, |jpeg| RetagCover::Embed(jpeg.clone())),
    });

    let layout = Layout::from_settings(settings);
    let (stem, extension) = organize::stem_and_extension(&absolute);
    let desired = layout.render(&FileFacts {
        tags: &applied,
        original_stem: &stem,
        extension: &extension,
    });

    let batch_id = journal::create_batch(pool, BatchKind::Review, None).await?;
    let mut report = BatchReport {
        batch_id,
        ..BatchReport::default()
    };
    let lock = LibraryLock::acquire(pool, "librarian").await?;
    let outcome = file(pool, roots, choice, &current, &desired, retag, batch_id).await;
    lock.release().await?;
    let to = match outcome {
        Ok(to) => to,
        Err(OpError::Database(error)) => return Err(error.into()),
        Err(error) => {
            report.skipped.push(SkippedFile {
                path: current.to_string(),
                reason: error.to_string(),
            });
            journal::finish_batch(pool, batch_id).await?;
            return Ok(report);
        }
    };
    report.done = 1;

    sqlx::query(
        "UPDATE track_identifications SET status = 'identified', score = COALESCE(?, score),
             recording_mbid = ?, release_mbid = ?
         WHERE track_id = ?",
    )
    .bind(score)
    .bind(&choice.recording_id)
    .bind(&choice.release_id)
    .bind(choice.track_id)
    .execute(pool)
    .await?;
    if settings.save_cover
        && let (Some(jpeg), Some(dir)) = (&cover, to.resolve(&roots.library).parent())
    {
        import::save_cover(dir, jpeg);
    }
    journal::finish_batch(pool, batch_id).await?;
    tracing::info!(track_id = choice.track_id, from = %current, %to, "release applied after review");
    Ok(report)
}

/// Moves and re-tags the file. Hold the library lock. Returns its path.
async fn file(
    pool: &sqlx::MySqlPool,
    roots: &Roots,
    choice: &Choice,
    current: &LibraryPath,
    desired: &str,
    retag: Option<Retag>,
    batch_id: u64,
) -> Result<LibraryPath, OpError> {
    let mut planned = HashSet::new();
    let to = loop {
        let candidate =
            organize::free_path(desired, Some(current.as_str()), &mut planned, |path| {
                std::fs::symlink_metadata(roots.library.join(path)).is_ok()
            });
        let Some(candidate) = candidate.map(LibraryPath::from_stored) else {
            // Already where the layout puts it.
            break current.clone();
        };
        if journal::is_free(pool, roots, &candidate).await? {
            break candidate;
        }
    };
    if to == *current && retag.is_none() {
        return Ok(to);
    }
    let operation = Operation {
        batch_id,
        subject: Subject::Existing(choice.track_id),
        from: Origin::Library(current.clone()),
        to: to.clone(),
        retag,
        expected_stamp: None,
    };
    journal::execute(pool, roots, operation).await?;
    Ok(to)
}
