// SPDX-License-Identifier: AGPL-3.0-or-later

//! Files prepared in the buffer zone (categories, labels, mood, sound
//! code) become tracks that have them from the moment they exist.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::time::Duration;

use common::{Mock, MockState, temp_roots, write_wav};
use heron_service::db::ScratchDatabase;
use sqlx::MySqlPool;

async fn file_id(pool: &MySqlPool, name: &str) -> u64 {
    sqlx::query_scalar("SELECT id FROM staged_files WHERE original_name = ?")
        .bind(name)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn insert(pool: &MySqlPool, sql: &'static str, name: &str) -> u64 {
    sqlx::query(sql)
        .bind(name)
        .execute(pool)
        .await
        .unwrap()
        .last_insert_id()
}

#[tokio::test]
async fn prepared_files_become_prepared_tracks() {
    let Some(db) = ScratchDatabase::create().await else {
        return;
    };
    let pool = &db.pool;
    let (_dir, roots) = temp_roots();
    for (n, name) in ["a.wav", "b.wav", "c.wav"].iter().enumerate() {
        write_wav(&roots.incoming.join("Album").join(name), n as u64 + 1);
    }
    let mock = Mock::start(MockState::default()).await;
    let staged = common::stage(pool, &roots, mock.endpoints(), Duration::ZERO).await;
    assert_eq!(staged.staged, 3);

    let gold = insert(pool, "INSERT INTO categories (name) VALUES (?)", "Gold").await;
    let new = insert(pool, "INSERT INTO categories (name) VALUES (?)", "New").await;
    let gone = insert(pool, "INSERT INTO categories (name) VALUES (?)", "Gone").await;
    let summer = insert(pool, "INSERT INTO labels (name) VALUES (?)", "summer").await;
    let (a, b, c) = (
        file_id(pool, "Album/a.wav").await,
        file_id(pool, "Album/b.wav").await,
        file_id(pool, "Album/c.wav").await,
    );
    // As the API prepares them: the whole group in Gold and New, with the
    // summer label and a mood; b taken out of New; c given a weight.
    for file in [a, b, c] {
        for (category, weight) in [(gold, 1.0), (new, 1.0), (gone, 1.0)] {
            sqlx::query(
                "INSERT INTO staged_file_categories (staged_file_id, category_id, weight)
                 VALUES (?, ?, ?)",
            )
            .bind(file)
            .bind(category)
            .bind(if file == c && category == gold {
                2.5
            } else {
                weight
            })
            .execute(pool)
            .await
            .unwrap();
        }
        sqlx::query("INSERT INTO staged_file_labels (staged_file_id, label_id) VALUES (?, ?)")
            .bind(file)
            .bind(summer)
            .execute(pool)
            .await
            .unwrap();
    }
    sqlx::query("UPDATE staged_files SET mood = 'calm' WHERE id IN (?, ?)")
        .bind(a)
        .bind(b)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("UPDATE staged_files SET sound_code = 'hard' WHERE id = ?")
        .bind(c)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM staged_file_categories WHERE staged_file_id = ? AND category_id = ?")
        .bind(b)
        .bind(new)
        .execute(pool)
        .await
        .unwrap();
    // Deleted after the files were put in it: left out.
    sqlx::query("DELETE FROM categories WHERE id = ?")
        .bind(gone)
        .execute(pool)
        .await
        .unwrap();

    let report = common::import_ready(pool, &roots, mock.endpoints()).await;
    assert_eq!((report.done, report.skipped.len()), (3, 0), "{report:?}");

    let track = |file: u64| async move {
        sqlx::query_scalar::<_, u64>("SELECT track_id FROM staged_files WHERE id = ?")
            .bind(file)
            .fetch_one(pool)
            .await
            .unwrap()
    };
    for (file, categories, mood, sound_code) in [
        (a, vec![("Gold", 1.0), ("New", 1.0)], Some("calm"), None),
        (b, vec![("Gold", 1.0)], Some("calm"), None),
        (c, vec![("Gold", 2.5), ("New", 1.0)], None, Some("hard")),
    ] {
        let track_id = track(file).await;
        let given: Vec<(String, f64)> = sqlx::query_as(
            "SELECT c.name, tc.weight FROM track_categories tc
             JOIN categories c ON c.id = tc.category_id WHERE tc.track_id = ? ORDER BY c.name",
        )
        .bind(track_id)
        .fetch_all(pool)
        .await
        .unwrap();
        let expected: Vec<(String, f64)> = categories
            .into_iter()
            .map(|(name, weight)| (name.to_owned(), weight))
            .collect();
        assert_eq!(given, expected, "categories of file {file}");
        let labels: Vec<u64> =
            sqlx::query_scalar("SELECT label_id FROM track_labels WHERE track_id = ?")
                .bind(track_id)
                .fetch_all(pool)
                .await
                .unwrap();
        assert_eq!(labels, [summer]);
        let programming: (Option<String>, Option<String>) =
            sqlx::query_as("SELECT mood, sound_code FROM track_programming WHERE track_id = ?")
                .bind(track_id)
                .fetch_one(pool)
                .await
                .unwrap();
        assert_eq!(
            programming,
            (mood.map(str::to_owned), sound_code.map(str::to_owned))
        );
    }

    // The tracks of the batch are the three imported.
    let in_batch: i64 = sqlx::query_scalar(
        "SELECT COUNT(DISTINCT track_id) FROM library_moves
         WHERE batch_id = ? AND from_root = 'staging'",
    )
    .bind(report.batch_id)
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(in_batch, 3);
}

async fn applied(pool: &MySqlPool, id: u64) -> (heron_library::Tags, String, String) {
    let (tags, path, status): (String, String, String) = sqlx::query_as(
        "SELECT CAST(applied_tags AS CHAR), planned_path, CAST(status AS CHAR)
         FROM staged_files WHERE id = ?",
    )
    .bind(id)
    .fetch_one(pool)
    .await
    .unwrap();
    (serde_json::from_str(&tags).unwrap(), path, status)
}

/// Edits as the API makes them: stored, and the ready file waits for the
/// librarian again.
async fn edit(pool: &MySqlPool, id: u64, edits: serde_json::Value) {
    sqlx::query(
        "UPDATE staged_files SET tag_edits = ?,
             status = IF(status = 'ready', 'identifying', status)
         WHERE id = ?",
    )
    .bind(edits.to_string())
    .bind(id)
    .execute(pool)
    .await
    .unwrap();
}

#[tokio::test]
async fn edited_tags_go_on_top_of_the_identification() {
    use common::{Album, Service, acoustid_result, fingerprint};
    use heron_core::LibrarySettings;
    use heron_librarian::staging::Stager;

    let Some(db) = ScratchDatabase::create().await else {
        return;
    };
    let pool = &db.pool;
    let (_dir, roots) = temp_roots();
    let album = Album::new(1, 10, false);
    let mut mock = MockState::default();
    let mut originals = Vec::new();
    for n in 0..2 {
        let path = roots.incoming.join(format!("Drop/{n}.wav"));
        write_wav(&path, n as u64 + 1);
        originals.push(std::fs::read(&path).unwrap());
        mock.acoustid.insert(
            fingerprint(&path),
            acoustid_result(0.97, &album.tracks[n].0, vec![album.acoustid_group(n)]),
        );
    }
    mock.releases
        .insert(album.release_id.clone(), album.musicbrainz());
    let mock = Mock::start(mock).await;
    sqlx::query("INSERT INTO settings (name, value) VALUES ('library', ?)")
        .bind(
            serde_json::to_string(&LibrarySettings {
                acoustid_key: Some("test-key".into()),
                save_cover: false,
                embed_cover: false,
                ..LibrarySettings::default()
            })
            .unwrap(),
        )
        .execute(pool)
        .await
        .unwrap();
    common::stage(pool, &roots, mock.endpoints(), Duration::ZERO).await;
    let (first, second) = (
        file_id(pool, "Drop/0.wav").await,
        file_id(pool, "Drop/1.wav").await,
    );
    let (tags, path, _) = applied(pool, first).await;
    assert_eq!(tags.title.as_deref(), Some("Song 1 of 1"));
    assert_eq!(path, "Band 1/Album 1/01 Song 1 of 1.wav");
    let lookups = mock.requests(Service::MusicBrainz).len();

    // A selection, then one of them; a field set to null is removed.
    for id in [first, second] {
        edit(
            pool,
            id,
            serde_json::json!({"album": "Live", "year": "2001"}),
        )
        .await;
    }
    edit(
        pool,
        first,
        serde_json::json!({"album": "Live", "year": "2001", "title": "Opening", "disc": null}),
    )
    .await;
    let mut stager = Stager::new(pool.clone(), roots.clone(), mock.endpoints());
    assert_eq!(stager.identify_once().await.unwrap().ready, 2);
    let (tags, path, status) = applied(pool, first).await;
    assert_eq!(status, "ready");
    assert_eq!(
        (
            tags.title.as_deref(),
            tags.album.as_deref(),
            tags.date.as_deref()
        ),
        (Some("Opening"), Some("Live"), Some("2001"))
    );
    assert_eq!(
        tags.artist.as_deref(),
        Some("Band 1"),
        "kept from the release"
    );
    assert_eq!(tags.disc, None);
    assert_eq!(path, "Band 1/Live/01 Opening.wav");
    let (tags, path, _) = applied(pool, second).await;
    assert_eq!(tags.title.as_deref(), Some("Song 2 of 1"));
    assert_eq!(path, "Band 1/Live/02 Song 2 of 1.wav");
    assert_eq!(
        mock.requests(Service::MusicBrainz).len(),
        lookups,
        "the release is not looked up again"
    );

    let report = common::import_ready(pool, &roots, mock.endpoints()).await;
    assert_eq!((report.done, report.skipped.len()), (2, 0), "{report:?}");
    let written = heron_library::tags::read(&roots.library.join("Band 1/Live/01 Opening.wav"))
        .unwrap()
        .tags;
    assert_eq!(
        (
            written.title.as_deref(),
            written.album.as_deref(),
            written.date.as_deref()
        ),
        (Some("Opening"), Some("Live"), Some("2001"))
    );

    // Undone: the files stay in the library, byte for byte as they came.
    heron_librarian::organize::undo(pool, &roots, report.batch_id)
        .await
        .unwrap();
    for (path, original) in [
        "Band 1/Live/01 Opening.wav",
        "Band 1/Live/02 Song 2 of 1.wav",
    ]
    .iter()
    .zip(&originals)
    {
        assert_eq!(
            &std::fs::read(roots.library.join(path)).unwrap(),
            original,
            "{path} byte for byte"
        );
    }
}
