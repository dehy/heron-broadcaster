// SPDX-License-Identifier: AGPL-3.0-or-later

//! Tags edited from the browser, written into the files and undone byte for
//! byte; programming details written into the files; MusicBrainz details.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::type_complexity)]

mod common;

use common::{
    Mock, MockState, assert_tracks_match_files, insert_track, snapshot, temp_roots, write_tagged,
};
use heron_core::LibraryPath;
use heron_core::library::{BatchReport, DetailsReport, FieldValues, TagField, TaskRequest};
use heron_librarian::tasks::TaskRunner;
use heron_library::Tags;
use heron_service::db::ScratchDatabase;
use serde_json::json;
use sqlx::MySqlPool;

fn tags(title: &str) -> Tags {
    Tags {
        title: Some(title.into()),
        artist: Some("Band".into()),
        album: Some("Album".into()),
        date: Some("1997-05-21".into()),
        track: Some(1),
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

/// Runs the next task, which must succeed, and returns its result.
async fn run<T: serde::de::DeserializeOwned>(runner: &TaskRunner, pool: &MySqlPool, id: u64) -> T {
    assert!(runner.run_next().await.unwrap());
    let (status, result, error): (String, Option<String>, Option<String>) = sqlx::query_as(
        "SELECT CAST(status AS CHAR), CAST(result AS CHAR), error FROM library_tasks WHERE id = ?",
    )
    .bind(id)
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(status, "done", "{error:?}");
    serde_json::from_str(&result.unwrap()).unwrap()
}

fn fields(values: &[(TagField, Option<&str>)]) -> FieldValues {
    values
        .iter()
        .map(|(field, value)| (*field, value.map(str::to_owned)))
        .collect()
}

fn read(library: &std::path::Path, path: &str, which: &[TagField]) -> FieldValues {
    heron_library::tags::read_fields(
        &LibraryPath::from_stored(path.to_owned()).resolve(library),
        which.iter().copied(),
    )
    .unwrap()
}

#[tokio::test]
async fn edited_tags_are_undone_byte_for_byte() {
    let Some(db) = ScratchDatabase::create().await else {
        return;
    };
    let pool = &db.pool;
    let (_dir, roots) = temp_roots();
    let library = &roots.library;
    let paths = ["a/one.wav", "a/two.wav", "b/three.wav"];
    let mut ids = Vec::new();
    for (seed, path) in paths.iter().enumerate() {
        write_tagged(&library.join(path), seed as u64, &tags(path));
        ids.push(insert_track(pool, library, path).await);
    }
    // What the tracks have besides their file.
    sqlx::query(
        "INSERT INTO track_analysis (track_id, analysis_version, params_hash, cue_in_ms, cue_out_ms)
         VALUES (?, 4, '', 0, 1000)",
    )
    .bind(ids[0])
    .execute(pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO categories (id, name) VALUES (1, 'Gold')")
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO track_categories (track_id, category_id, weight) VALUES (?, 1, 1)")
        .bind(ids[0])
        .execute(pool)
        .await
        .unwrap();
    let original = snapshot(library);
    let runner = TaskRunner::new(
        pool.clone(),
        roots.clone(),
        Mock::start(MockState::default()).await.endpoints(),
    );

    let set = fields(&[
        (TagField::Genre, Some("Jazz")),
        (TagField::Year, Some("1999")),
        (TagField::Album, None),
    ]);
    let edit = ask(
        pool,
        &TaskRequest::EditTags {
            track_ids: ids.clone(),
            set: set.clone(),
            from_details: Vec::new(),
        },
    )
    .await;
    let report: BatchReport = run(&runner, pool, edit).await;
    assert_eq!((report.done, report.unchanged), (3, 0), "{report:?}");
    assert!(report.skipped.is_empty(), "{report:?}");
    for path in paths {
        assert_eq!(
            read(
                library,
                path,
                &[TagField::Genre, TagField::Year, TagField::Album]
            ),
            set
        );
        // Fields left alone keep each track's value.
        assert_eq!(
            read(library, path, &[TagField::Title])[&TagField::Title].as_deref(),
            Some(path)
        );
    }
    // Not moved, same tracks, read again by the librarian.
    let tracks: Vec<(u64, String, Option<String>, Option<u16>, Option<String>)> =
        sqlx::query_as("SELECT id, path, genre, year, album FROM tracks ORDER BY id")
            .fetch_all(pool)
            .await
            .unwrap();
    for ((id, path, genre, year, album), (expected_id, expected_path)) in
        tracks.iter().zip(ids.iter().zip(paths))
    {
        assert_eq!((id, path.as_str()), (expected_id, expected_path));
        assert_eq!(
            (genre.as_deref(), *year, album),
            (Some("Jazz"), Some(1999), &None)
        );
    }
    let kept: (i64, i64) = sqlx::query_as(
        "SELECT (SELECT COUNT(*) FROM track_analysis), (SELECT COUNT(*) FROM track_categories)",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(kept, (1, 1), "analysis and categories are kept");
    let restorable: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM track_tag_changes WHERE restore IS NOT NULL")
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!(restorable, 3, "each change can be rebuilt byte for byte");
    assert_tracks_match_files(pool, library).await;

    // The same values again change nothing.
    let again = ask(
        pool,
        &TaskRequest::EditTags {
            track_ids: ids.clone(),
            set,
            from_details: Vec::new(),
        },
    )
    .await;
    let report: BatchReport = run(&runner, pool, again).await;
    assert_eq!((report.done, report.unchanged), (0, 3), "{report:?}");

    let undo = ask(
        pool,
        &TaskRequest::Undo {
            batch_id: report.batch_id - 1,
        },
    )
    .await;
    let report: BatchReport = run(&runner, pool, undo).await;
    assert_eq!(report.done, 3, "{report:?}");
    assert_eq!(snapshot(library), original, "byte for byte");
    assert_tracks_match_files(pool, library).await;
    let album: Option<String> = sqlx::query_scalar("SELECT album FROM tracks WHERE id = ?")
        .bind(ids[0])
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(album.as_deref(), Some("Album"));
    db.drop().await;
}

#[tokio::test]
async fn an_edit_changed_again_is_undone_field_by_field() {
    let Some(db) = ScratchDatabase::create().await else {
        return;
    };
    let pool = &db.pool;
    let (_dir, roots) = temp_roots();
    let library = &roots.library;
    write_tagged(&library.join("a.wav"), 1, &tags("A"));
    let id = insert_track(pool, library, "a.wav").await;
    let runner = TaskRunner::new(
        pool.clone(),
        roots.clone(),
        Mock::start(MockState::default()).await.endpoints(),
    );
    let mut batches = Vec::new();
    for set in [
        fields(&[(TagField::Genre, Some("Jazz"))]),
        fields(&[(TagField::Year, Some("2001"))]),
    ] {
        let task = ask(
            pool,
            &TaskRequest::EditTags {
                track_ids: vec![id],
                set,
                from_details: Vec::new(),
            },
        )
        .await;
        let report: BatchReport = run(&runner, pool, task).await;
        assert_eq!(report.done, 1);
        batches.push(report.batch_id);
    }
    // Undoing the first edit keeps the second one.
    let undo = ask(
        pool,
        &TaskRequest::Undo {
            batch_id: batches[0],
        },
    )
    .await;
    let report: BatchReport = run(&runner, pool, undo).await;
    assert_eq!(report.done, 1, "{report:?}");
    assert_eq!(
        read(library, "a.wav", &[TagField::Genre, TagField::Year]),
        fields(&[(TagField::Genre, None), (TagField::Year, Some("2001"))])
    );
    db.drop().await;
}

#[tokio::test]
async fn programming_details_are_written_into_the_files() {
    let Some(db) = ScratchDatabase::create().await else {
        return;
    };
    let pool = &db.pool;
    let (_dir, roots) = temp_roots();
    let library = &roots.library;
    write_tagged(&library.join("a.wav"), 1, &tags("A"));
    write_tagged(&library.join("b.wav"), 2, &tags("B"));
    let a = insert_track(pool, library, "a.wav").await;
    let b = insert_track(pool, library, "b.wav").await;
    sqlx::query(
        "INSERT INTO track_musicbrainz (track_id, release_id, recording_id, record_label, isrc)
         VALUES (?, 'r', 'rec', 'Blue Note', 'USBN20100001')",
    )
    .bind(a)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO track_programming (track_id, bpm, mood) VALUES (?, 98.4, 'calm')")
        .bind(a)
        .execute(pool)
        .await
        .unwrap();
    let runner = TaskRunner::new(
        pool.clone(),
        roots.clone(),
        Mock::start(MockState::default()).await.endpoints(),
    );
    let details = [
        TagField::Isrc,
        TagField::RecordLabel,
        TagField::Bpm,
        TagField::Mood,
    ];
    let task = ask(
        pool,
        &TaskRequest::EditTags {
            track_ids: vec![a, b],
            set: FieldValues::new(),
            from_details: details.to_vec(),
        },
    )
    .await;
    let report: BatchReport = run(&runner, pool, task).await;
    // Heron knows nothing about b: its file is left alone.
    assert_eq!((report.done, report.unchanged), (1, 1), "{report:?}");
    assert_eq!(
        read(library, "a.wav", &details),
        fields(&[
            (TagField::Isrc, Some("USBN20100001")),
            (TagField::RecordLabel, Some("Blue Note")),
            // ID3v2 keeps whole tempos.
            (TagField::Bpm, Some("98")),
            (TagField::Mood, Some("calm")),
        ])
    );
    db.drop().await;
}

#[tokio::test]
async fn musicbrainz_details_of_identified_tracks() {
    let Some(db) = ScratchDatabase::create().await else {
        return;
    };
    let pool = &db.pool;
    let (_dir, roots) = temp_roots();
    let library = &roots.library;
    let mut ids = Vec::new();
    for (seed, path) in ["a.wav", "b.wav", "c.wav"].iter().enumerate() {
        write_tagged(&library.join(path), seed as u64, &tags(path));
        ids.push(insert_track(pool, library, path).await);
    }
    // a and b were identified on the same release; c was not identified.
    for (id, recording) in [(ids[0], "rec-a"), (ids[1], "rec-b")] {
        sqlx::query(
            "UPDATE tracks SET musicbrainz_release_id = 'rel', musicbrainz_recording_id = ?
             WHERE id = ?",
        )
        .bind(recording)
        .bind(id)
        .execute(pool)
        .await
        .unwrap();
    }
    let mut state = MockState::default();
    state.releases.insert(
        "rel".into(),
        json!({
            "id": "rel", "title": "Hits", "date": "2004",
            "label-info": [{"catalog-number": "BN 1", "label": {"name": "Blue Note"}}],
            "release-group": {"first-release-date": "2004"},
            "media": [{"tracks": [
                {"recording": {"id": "rec-a", "isrcs": ["USBN20100001"], "first-release-date": "1964",
                               "genres": [{"name": "jazz", "count": 3}]}},
                {"recording": {"id": "rec-b"}}
            ]}]
        }),
    );
    let mock = Mock::start(state).await;
    let runner = TaskRunner::new(pool.clone(), roots.clone(), mock.endpoints());
    assert!(
        !heron_librarian::musicbrainz::background_enabled(pool)
            .await
            .unwrap()
    );

    let task = ask(pool, &TaskRequest::MusicbrainzDetails { all: false }).await;
    let report: DetailsReport = run(&runner, pool, task).await;
    assert_eq!(
        (report.releases, report.tracks, report.failed),
        (1, 2, 0),
        "{report:?}"
    );
    let rows: Vec<(
        u64,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<u16>,
        Option<String>,
    )> = sqlx::query_as(
        "SELECT track_id, album, record_label, isrc, first_release_year, CAST(genres AS CHAR)
             FROM track_musicbrainz ORDER BY track_id",
    )
    .fetch_all(pool)
    .await
    .unwrap();
    assert_eq!(
        rows,
        [
            (
                ids[0],
                Some("Hits".into()),
                Some("Blue Note".into()),
                Some("USBN20100001".into()),
                Some(1964),
                Some(r#"["jazz"]"#.into())
            ),
            (
                ids[1],
                Some("Hits".into()),
                Some("Blue Note".into()),
                None,
                Some(2004),
                None
            ),
        ]
    );
    assert!(
        heron_librarian::musicbrainz::background_enabled(pool)
            .await
            .unwrap()
    );

    // Fetched already: only `all` looks them up again.
    let task = ask(pool, &TaskRequest::MusicbrainzDetails { all: false }).await;
    let report: DetailsReport = run(&runner, pool, task).await;
    assert_eq!(report.releases, 0, "{report:?}");
    let task = ask(pool, &TaskRequest::MusicbrainzDetails { all: true }).await;
    let report: DetailsReport = run(&runner, pool, task).await;
    assert_eq!(report.releases, 1, "{report:?}");
    db.drop().await;
}
