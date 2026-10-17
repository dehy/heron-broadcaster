// SPDX-License-Identifier: AGPL-3.0-or-later

//! Library tasks asked through the API: an organize dry run, its apply and
//! its undo; a release picked on the review screen, and its undo.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use common::{Album, Mock, MockState, insert_track, snapshot, temp_roots, write_tagged};
use heron_core::library::{BatchReport, OrganizePlan, TaskRequest};
use heron_librarian::tasks::TaskRunner;
use heron_library::Tags;
use heron_service::db::ScratchDatabase;
use serde_json::json;
use sqlx::MySqlPool;

fn tags(artist: &str, album: &str, track: u32, title: &str) -> Tags {
    Tags {
        title: Some(title.into()),
        artist: Some(artist.into()),
        album: Some(album.into()),
        track: Some(track),
        ..Tags::default()
    }
}

async fn ask(pool: &MySqlPool, request: &TaskRequest) -> u64 {
    sqlx::query("INSERT INTO library_tasks (kind, params) VALUES (?, ?)")
        .bind(request.kind())
        .bind(serde_json::to_string(request).unwrap())
        .execute(pool)
        .await
        .unwrap()
        .last_insert_id()
}

/// Runs the next task and returns its status, result and error.
async fn run(
    runner: &TaskRunner,
    pool: &MySqlPool,
    id: u64,
) -> (String, Option<serde_json::Value>, Option<String>) {
    assert!(runner.run_next().await.unwrap());
    let (status, result, error): (String, Option<String>, Option<String>) = sqlx::query_as(
        "SELECT CAST(status AS CHAR), CAST(result AS CHAR), error FROM library_tasks WHERE id = ?",
    )
    .bind(id)
    .fetch_one(pool)
    .await
    .unwrap();
    (
        status,
        result.map(|result| serde_json::from_str(&result).unwrap()),
        error,
    )
}

#[tokio::test]
async fn organize_through_tasks() {
    let Some(db) = ScratchDatabase::create().await else {
        return;
    };
    let pool = &db.pool;
    let (_dir, roots) = temp_roots();
    let library = &roots.library;
    write_tagged(
        &library.join("misc/a.wav"),
        1,
        &tags("Band", "Album", 1, "One"),
    );
    write_tagged(
        &library.join("misc/b.wav"),
        2,
        &tags("Band", "Album", 2, "Two"),
    );
    insert_track(pool, library, "misc/a.wav").await;
    insert_track(pool, library, "misc/b.wav").await;
    let original = snapshot(library);
    let runner = TaskRunner::new(
        pool.clone(),
        roots.clone(),
        Mock::start(MockState::default()).await.endpoints(),
    );
    assert!(!runner.run_next().await.unwrap(), "nothing to do");

    let plan_task = ask(pool, &TaskRequest::OrganizePlan).await;
    let (status, result, _) = run(&runner, pool, plan_task).await;
    assert_eq!(status, "done");
    let plan: OrganizePlan = serde_json::from_value(result.unwrap()).unwrap();
    assert_eq!(plan.moves.len(), 2);
    assert_eq!(snapshot(library), original, "the dry run touched files");

    let apply = TaskRequest::OrganizeApply {
        plan_task_id: plan_task,
    };
    let apply_task = ask(pool, &apply).await;
    let (status, result, error) = run(&runner, pool, apply_task).await;
    assert_eq!(status, "done", "{error:?}");
    let report: BatchReport = serde_json::from_value(result.unwrap()).unwrap();
    assert_eq!((report.done, report.skipped.len()), (2, 0));
    assert!(library.join("Band/Album/01 One.wav").exists());

    // A dry run is applied once.
    let again = ask(pool, &apply).await;
    let (status, _, error) = run(&runner, pool, again).await;
    assert_eq!(status, "failed");
    assert!(error.unwrap().contains("applied already"));
    // Only dry runs can be applied.
    let wrong = ask(
        pool,
        &TaskRequest::OrganizeApply {
            plan_task_id: apply_task,
        },
    )
    .await;
    assert_eq!(run(&runner, pool, wrong).await.0, "failed");

    let undo = ask(
        pool,
        &TaskRequest::Undo {
            batch_id: report.batch_id,
        },
    )
    .await;
    let (status, _, error) = run(&runner, pool, undo).await;
    assert_eq!(status, "done", "{error:?}");
    assert_eq!(snapshot(library), original, "undo restores every file");
    db.drop().await;
}

#[tokio::test]
async fn a_release_picked_on_the_review_screen() {
    let Some(db) = ScratchDatabase::create().await else {
        return;
    };
    let pool = &db.pool;
    let (_dir, roots) = temp_roots();
    let library = &roots.library;
    let album = Album::new(7, 3, false);
    let (recording, title) = album.tracks[1].clone();
    write_tagged(
        &library.join("loose/track.wav"),
        3,
        &tags("someone", "something", 9, "song"),
    );
    let track_id = insert_track(pool, library, "loose/track.wav").await;
    let candidates = json!([
        {"score": 0.8, "recording_id": recording, "release_id": album.release_id,
         "release_title": album.title, "disc": 1, "track": 2},
    ]);
    sqlx::query(
        "INSERT INTO track_identifications (track_id, status, reason, score, candidates)
         VALUES (?, 'needs_review', 'best AcoustID score 0.80 is below 0.90', 0.8, ?)",
    )
    .bind(track_id)
    .bind(candidates.to_string())
    .execute(pool)
    .await
    .unwrap();

    let mut state = MockState::default();
    state
        .releases
        .insert(album.release_id.clone(), album.musicbrainz());
    state
        .covers
        .insert(album.release_id.clone(), b"\xff\xd8 cover".to_vec());
    let mock = Mock::start(state).await;
    let runner = TaskRunner::new(pool.clone(), roots.clone(), mock.endpoints());

    let pick = TaskRequest::ReviewApply {
        track_id,
        release_id: album.release_id.clone(),
        recording_id: recording.clone(),
        disc: Some(1),
        track: Some(2),
    };
    let task = ask(pool, &pick).await;
    let (status, result, error) = run(&runner, pool, task).await;
    assert_eq!(status, "done", "{error:?}");
    let report: BatchReport = serde_json::from_value(result.unwrap()).unwrap();
    assert_eq!(report.done, 1, "{report:?}");

    let expected = format!("Band 7/Album 7/02 {title}.wav");
    let (path, status): (String, String) = sqlx::query_as(
        "SELECT t.path, CAST(i.status AS CHAR) FROM tracks t
         JOIN track_identifications i ON i.track_id = t.id WHERE t.id = ?",
    )
    .bind(track_id)
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(
        (path.as_str(), status.as_str()),
        (expected.as_str(), "identified")
    );
    let written = heron_library::tags::read(&library.join(&expected))
        .unwrap()
        .tags;
    assert_eq!(written.title.as_deref(), Some(title.as_str()));
    assert_eq!(written.album.as_deref(), Some("Album 7"));
    assert!(library.join("Band 7/Album 7/cover.jpg").exists());

    // Reviewed tracks are not reviewed twice.
    let twice = ask(pool, &pick).await;
    let (status, _, error) = run(&runner, pool, twice).await;
    assert_eq!(status, "failed");
    assert!(error.unwrap().contains("does not need review"));

    let undo = ask(
        pool,
        &TaskRequest::Undo {
            batch_id: report.batch_id,
        },
    )
    .await;
    let (status, _, error) = run(&runner, pool, undo).await;
    assert_eq!(status, "done", "{error:?}");
    // The file is back, with its own tags (the cover saved stays).
    let restored = snapshot(library);
    assert_eq!(
        restored.keys().collect::<Vec<_>>(),
        ["Band 7/Album 7/cover.jpg", "loose/track.wav"]
    );
    let own = heron_library::tags::read(&library.join("loose/track.wav"))
        .unwrap()
        .tags;
    assert_eq!(own, tags("someone", "something", 9, "song"));
    common::assert_tracks_match_files(pool, library).await;
    let status: String = sqlx::query_scalar(
        "SELECT CAST(status AS CHAR) FROM track_identifications WHERE track_id = ?",
    )
    .bind(track_id)
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(status, "needs_review");
    db.drop().await;
}
