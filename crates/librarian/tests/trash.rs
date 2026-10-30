// SPDX-License-Identifier: AGPL-3.0-or-later

//! Acceptance: deleting tracks moves their files to the trash, restoring
//! brings them back byte for byte with their analysis, categories and
//! history, and the trash is purged after the retention period (time faked
//! by moving the deletion back), keeping the plays' artist and title.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use common::{insert_track, path_buf, snapshot, temp_roots, write_wav};
use heron_librarian::{organize, trash};
use heron_service::db::ScratchDatabase;
use sqlx::MySqlPool;

async fn count(pool: &MySqlPool, query: &str) -> i64 {
    sqlx::query_scalar(sqlx::AssertSqlSafe(query.to_owned()))
        .fetch_one(pool)
        .await
        .unwrap()
}

#[tokio::test]
async fn delete_restore_and_purge() {
    let Some(db) = ScratchDatabase::create().await else {
        return;
    };
    let pool = &db.pool;
    let (_dir, roots) = temp_roots();

    // 50 tracks in 5 albums with a cover each, analyzed, in a category,
    // played once; and a track whose file is gone.
    let category = sqlx::query("INSERT INTO categories (name) VALUES ('Gold')")
        .execute(pool)
        .await
        .unwrap()
        .last_insert_id();
    let mut ids = Vec::new();
    for album in 1_u32..=5 {
        for n in 1..=10 {
            let path = format!("Band/Album {album}/{n:02}.wav");
            write_wav(&path_buf(&roots.library, &path), u64::from(album * 100 + n));
            ids.push(insert_track(pool, &roots.library, &path).await);
        }
        std::fs::write(
            roots.library.join(format!("Band/Album {album}/cover.jpg")),
            b"jpeg",
        )
        .unwrap();
    }
    // A track left in the first album: its cover stays.
    write_wav(&roots.library.join("Band/Album 1/keep.wav"), 1);
    let kept = insert_track(pool, &roots.library, "Band/Album 1/keep.wav").await;
    let missing = sqlx::query(
        "INSERT INTO tracks (path, title, missing_since) VALUES ('Gone/x.wav', 'Gone', NOW(6))",
    )
    .execute(pool)
    .await
    .unwrap()
    .last_insert_id();
    ids.push(missing);
    for &id in &ids {
        sqlx::query(
            "INSERT INTO track_analysis (track_id, analysis_version, params_hash, cue_in_ms, cue_out_ms)
             VALUES (?, 1, '', 0, 8000)",
        )
        .bind(id)
        .execute(pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO track_categories (track_id, category_id, weight) VALUES (?, ?, 1)",
        )
        .bind(id)
        .bind(category)
        .execute(pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO play_history (station_id, track_id, artist, title, started_at, deck, source)
             VALUES (1, ?, 'Band', ?, NOW(6), 'a', 'rotation')",
        )
        .bind(id)
        .bind(format!("Song {id}"))
        .execute(pool)
        .await
        .unwrap();
    }
    let before = snapshot(&roots.library);

    let report = trash::delete(pool, &roots, &ids).await.unwrap();
    assert_eq!((report.done, report.skipped.len()), (51, 0), "{report:?}");
    let trash_dir = roots
        .library
        .join(format!(".heron-trash/{}", report.batch_id));
    assert!(trash_dir.join("Band/Album 2/05.wav").is_file());
    assert!(
        trash_dir.join("Band/Album 2/cover.jpg").is_file(),
        "the cover went along"
    );
    assert!(
        !roots.library.join("Band/Album 2").exists(),
        "emptied folders are removed"
    );
    assert!(
        roots.library.join("Band/Album 1/cover.jpg").is_file(),
        "a track is left there"
    );
    assert_eq!(
        count(
            pool,
            "SELECT COUNT(*) FROM tracks WHERE deleted_at IS NOT NULL AND missing_since IS NOT NULL"
        )
        .await,
        51
    );
    // Asking again changes nothing.
    let again = trash::delete(pool, &roots, &ids[..3]).await.unwrap();
    assert_eq!((again.done, again.skipped.len()), (0, 3));

    let restored = organize::undo(pool, &roots, report.batch_id).await.unwrap();
    assert_eq!(
        (restored.done, restored.skipped.len()),
        (55, 0),
        "{restored:?}"
    );
    assert_eq!(snapshot(&roots.library), before, "restored byte for byte");
    assert_eq!(
        count(
            pool,
            "SELECT COUNT(*) FROM tracks WHERE deleted_at IS NOT NULL"
        )
        .await,
        0
    );
    assert_eq!(
        count(
            pool,
            "SELECT COUNT(*) FROM tracks WHERE missing_since IS NOT NULL"
        )
        .await,
        1,
        "only the track that had no file is missing"
    );
    let missing_path: String = sqlx::query_scalar("SELECT path FROM tracks WHERE id = ?")
        .bind(missing)
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(missing_path, "Gone/x.wav");
    for table in ["track_analysis", "track_categories", "play_history"] {
        assert_eq!(
            count(
                pool,
                &format!("SELECT COUNT(*) FROM {table} WHERE track_id IS NOT NULL")
            )
            .await,
            51,
            "{table} kept"
        );
    }

    // Deleted again, then purged once the retention period is over.
    let report = trash::delete(pool, &roots, &ids).await.unwrap();
    assert_eq!(report.done, 51);
    let nothing = trash::purge(pool, &roots, Some(30)).await.unwrap();
    assert_eq!(nothing.tracks, 0, "too recent");
    sqlx::query(
        "UPDATE tracks SET deleted_at = deleted_at - INTERVAL 31 DAY WHERE deleted_at IS NOT NULL",
    )
    .execute(pool)
    .await
    .unwrap();
    sqlx::query("UPDATE library_batches SET created_at = created_at - INTERVAL 31 DAY WHERE kind = 'delete'")
        .execute(pool)
        .await
        .unwrap();
    let purged = trash::purge(pool, &roots, Some(30)).await.unwrap();
    assert_eq!(purged.tracks, 51);
    assert_eq!(purged.files, 54, "50 tracks' files and 4 covers");
    assert!(
        !roots
            .library
            .join(".heron-trash")
            .join(report.batch_id.to_string())
            .exists()
    );
    assert_eq!(count(pool, "SELECT COUNT(*) FROM tracks").await, 1);
    let kept_path: String = sqlx::query_scalar("SELECT path FROM tracks WHERE id = ?")
        .bind(kept)
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(kept_path, "Band/Album 1/keep.wav");
    assert_eq!(
        count(pool, "SELECT COUNT(*) FROM play_history WHERE track_id IS NULL AND artist = 'Band' AND title LIKE 'Song %'").await,
        51,
        "the plays keep their artist and title"
    );
    assert_eq!(count(pool, "SELECT COUNT(*) FROM track_analysis").await, 0);
    common::assert_tracks_match_files(pool, &roots.library).await;
    db.drop().await;
}
