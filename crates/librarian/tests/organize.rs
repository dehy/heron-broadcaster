// SPDX-License-Identifier: AGPL-3.0-or-later

//! Acceptance: a dry run of organizing an existing library lists every move
//! and touches nothing; applying it then undoing it restores the original
//! paths and files byte for byte; moving keeps analysis, categories and
//! history.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use common::{insert_track, snapshot, temp_roots, write_tagged, write_wav};
use heron_core::LibrarySettings;
use heron_librarian::layout::Layout;
use heron_librarian::organize;
use heron_library::Tags;
use heron_service::db::ScratchDatabase;

fn tags(artist: &str, album: &str, track: u32, title: &str) -> Tags {
    Tags {
        title: Some(title.into()),
        artist: Some(artist.into()),
        album: Some(album.into()),
        track: Some(track),
        ..Tags::default()
    }
}

async fn paths(pool: &sqlx::MySqlPool) -> Vec<(u64, String)> {
    sqlx::query_as("SELECT id, path FROM tracks ORDER BY id")
        .fetch_all(pool)
        .await
        .unwrap()
}

#[tokio::test]
async fn dry_run_apply_and_undo() {
    let Some(db) = ScratchDatabase::create().await else {
        return;
    };
    let pool = &db.pool;
    let (_dir, roots) = temp_roots();
    let library = &roots.library;

    let files = [
        (
            "incoming stuff/a.wav",
            tags("Miles Davis", "Kind of Blue", 1, "So What"),
        ),
        (
            "incoming stuff/b.wav",
            tags("Miles Davis", "Kind of Blue", 2, "Freddie Freeloader"),
        ),
        ("misc/AC-DC - tnt.wav", tags("AC/DC", "T.N.T.", 7, "T.N.T.")),
        // Two files that want the same name.
        ("x/dup1.wav", tags("Band", "Album", 3, "Same")),
        ("y/dup2.wav", tags("Band", "Album", 3, "Same")),
        // Already in place.
        ("Band/Album/01 First.wav", tags("Band", "Album", 1, "First")),
    ];
    for (seed, (path, tags)) in files.iter().enumerate() {
        write_tagged(&library.join(path), seed as u64, tags);
    }
    // No tags at all.
    write_wav(&library.join("loose/track05.wav"), 99);
    // A file that is not a track (cover art, playlists): never touched.
    std::fs::write(library.join("incoming stuff/cover.jpg"), b"jpeg").unwrap();

    let mut ids = Vec::new();
    for (path, _) in &files {
        ids.push(insert_track(pool, library, path).await);
    }
    ids.push(insert_track(pool, library, "loose/track05.wav").await);
    // Analysis, categories and history of the first track.
    sqlx::query(
        "INSERT INTO track_analysis (track_id, analysis_version, params_hash, cue_in_ms, cue_out_ms)
         VALUES (?, 2, '', 10, 7990)",
    )
    .bind(ids[0])
    .execute(pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO categories (id, name) VALUES (1, 'Jazz')")
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO track_categories (track_id, category_id) VALUES (?, 1)")
        .bind(ids[0])
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO play_history (station_id, track_id, started_at, deck, source)
         VALUES (1, ?, NOW(6), 'a', 'rotation')",
    )
    .bind(ids[0])
    .execute(pool)
    .await
    .unwrap();

    let original_files = snapshot(library);
    let original_paths = paths(pool).await;
    let layout = Layout::parse(LibrarySettings::DEFAULT_LAYOUT).unwrap();

    // Dry run: every move listed, nothing touched.
    let plan = organize::plan(pool, &roots, &layout).await.unwrap();
    let mut planned: Vec<(&str, &str)> = plan
        .moves
        .iter()
        .map(|m| (m.from.as_str(), m.to.as_str()))
        .collect();
    planned.sort();
    assert_eq!(
        planned,
        [
            (
                "incoming stuff/a.wav",
                "Miles Davis/Kind of Blue/01 So What.wav"
            ),
            (
                "incoming stuff/b.wav",
                "Miles Davis/Kind of Blue/02 Freddie Freeloader.wav"
            ),
            (
                "loose/track05.wav",
                "Unknown Artist/Unknown Album/track05.wav"
            ),
            ("misc/AC-DC - tnt.wav", "AC_DC/T.N.T/07 T.N.T.wav"),
            ("x/dup1.wav", "Band/Album/03 Same.wav"),
            ("y/dup2.wav", "Band/Album/03 Same (2).wav"),
        ]
    );
    assert_eq!(plan.unchanged, 1);
    assert_eq!(
        snapshot(library),
        original_files,
        "the dry run touched files"
    );
    assert_eq!(
        paths(pool).await,
        original_paths,
        "the dry run touched the database"
    );

    // Apply.
    let report = organize::apply(pool, &roots, &plan).await.unwrap();
    assert_eq!((report.done, report.skipped.len()), (6, 0), "{report:?}");
    let moved = snapshot(library);
    assert_eq!(moved.len(), original_files.len(), "no file lost or added");
    assert_eq!(
        moved.get("Miles Davis/Kind of Blue/01 So What.wav"),
        original_files.get("incoming stuff/a.wav"),
        "moved byte for byte"
    );
    assert!(
        moved.contains_key("incoming stuff/cover.jpg"),
        "other files stay"
    );
    assert!(!library.join("misc").exists(), "empty folders are removed");
    common::assert_tracks_match_files(pool, library).await;

    // The track kept its ID, so its analysis, categories and history.
    let kept: (i64, i64, i64) = sqlx::query_as(
        "SELECT (SELECT COUNT(*) FROM track_analysis WHERE track_id = ?),
                (SELECT COUNT(*) FROM track_categories WHERE track_id = ?),
                (SELECT COUNT(*) FROM play_history WHERE track_id = ?)",
    )
    .bind(ids[0])
    .bind(ids[0])
    .bind(ids[0])
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(kept, (1, 1, 1));

    // Organizing again finds nothing to do.
    let again = organize::plan(pool, &roots, &layout).await.unwrap();
    assert!(again.moves.is_empty(), "{:?}", again.moves);

    // Undo: the original paths and files, byte for byte.
    let undone = organize::undo(pool, &roots, report.batch_id).await.unwrap();
    assert_eq!((undone.done, undone.skipped.len()), (6, 0), "{undone:?}");
    assert_eq!(snapshot(library), original_files);
    assert_eq!(paths(pool).await, original_paths);
    common::assert_tracks_match_files(pool, library).await;

    // A batch is undone once.
    assert!(matches!(
        organize::undo(pool, &roots, report.batch_id).await,
        Err(organize::Error::AlreadyUndone { .. })
    ));
    db.drop().await;
}

#[tokio::test]
async fn undo_never_overwrites() {
    let Some(db) = ScratchDatabase::create().await else {
        return;
    };
    let pool = &db.pool;
    let (_dir, roots) = temp_roots();
    let library = &roots.library;
    write_tagged(
        &library.join("old/a.wav"),
        1,
        &tags("Band", "Album", 1, "One"),
    );
    insert_track(pool, library, "old/a.wav").await;
    let layout = Layout::parse(LibrarySettings::DEFAULT_LAYOUT).unwrap();
    let plan = organize::plan(pool, &roots, &layout).await.unwrap();
    let report = organize::apply(pool, &roots, &plan).await.unwrap();
    assert_eq!(report.done, 1);

    // Someone puts another file where the track used to be.
    std::fs::create_dir_all(library.join("old")).unwrap();
    std::fs::write(library.join("old/a.wav"), b"someone else's file").unwrap();
    let undone = organize::undo(pool, &roots, report.batch_id).await.unwrap();
    assert_eq!((undone.done, undone.skipped.len()), (0, 1), "{undone:?}");
    assert_eq!(
        std::fs::read(library.join("old/a.wav")).unwrap(),
        b"someone else's file"
    );
    assert!(library.join("Band/Album/01 One.wav").exists());
    common::assert_tracks_match_files(pool, library).await;
    db.drop().await;
}

#[tokio::test]
async fn tags_kept_in_heron_file_tracks_without_touching_tags() {
    let Some(db) = ScratchDatabase::create().await else {
        return;
    };
    let pool = &db.pool;
    let (_dir, roots) = temp_roots();
    let library = &roots.library;
    write_tagged(
        &library.join("old/a.wav"),
        1,
        &tags("Band", "Albm", 1, "One"),
    );
    let track = insert_track(pool, library, "old/a.wav").await;
    // The album fixed in Heron (write_tags off), not in the file.
    sqlx::query(
        "INSERT INTO track_tag_overrides (track_id, field, value) VALUES (?, 'album', 'Album')",
    )
    .bind(track)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query("UPDATE tracks SET album = 'Album' WHERE id = ?")
        .bind(track)
        .execute(pool)
        .await
        .unwrap();
    let before = heron_library::tags::read(&library.join("old/a.wav"))
        .unwrap()
        .tags;

    let layout = Layout::parse(LibrarySettings::DEFAULT_LAYOUT).unwrap();
    let plan = organize::plan(pool, &roots, &layout).await.unwrap();
    assert_eq!(plan.moves.len(), 1, "{plan:?}");
    assert_eq!(plan.moves[0].to, "Band/Album/01 One.wav");
    let report = organize::apply(pool, &roots, &plan).await.unwrap();
    assert_eq!(report.done, 1, "{report:?}");

    let moved = library.join("Band/Album/01 One.wav");
    assert_eq!(heron_library::tags::read(&moved).unwrap().tags, before);
    let album: Option<String> = sqlx::query_scalar("SELECT album FROM tracks WHERE id = ?")
        .bind(track)
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(
        album.as_deref(),
        Some("Album"),
        "the override stays applied"
    );
    db.drop().await;
}
