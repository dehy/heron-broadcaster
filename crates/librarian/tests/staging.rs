// SPDX-License-Identifier: AGPL-3.0-or-later

//! The buffer zone: duplicates and unplayable files are flagged before
//! anything is imported, and a release picked by hand (or the file's own
//! tags) gives the tags and the path the file is imported with.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::time::Duration;

use common::{
    Album, Mock, MockState, acoustid_result, fingerprint, insert_track, temp_roots, write_tagged,
    write_wav,
};
use heron_core::LibrarySettings;
use heron_library::Tags;
use heron_service::db::ScratchDatabase;
use sqlx::MySqlPool;

async fn flags(pool: &MySqlPool, name: &str) -> Vec<String> {
    let flags: Option<String> =
        sqlx::query_scalar("SELECT CAST(flags AS CHAR) FROM staged_files WHERE original_name = ?")
            .bind(name)
            .fetch_one(pool)
            .await
            .unwrap();
    flags
        .map(|json| serde_json::from_str(&json).unwrap())
        .unwrap_or_default()
}

async fn planned(pool: &MySqlPool, name: &str) -> (String, String) {
    sqlx::query_as(
        "SELECT planned_path, CAST(status AS CHAR) FROM staged_files WHERE original_name = ?",
    )
    .bind(name)
    .fetch_one(pool)
    .await
    .unwrap()
}

#[tokio::test]
async fn flags_and_choices() {
    let Some(db) = ScratchDatabase::create().await else {
        return;
    };
    let pool = &db.pool;
    let (_dir, roots) = temp_roots();

    // In the library already, with its fingerprint.
    write_wav(&roots.library.join("Old/song.wav"), 1);
    let existing = insert_track(pool, &roots.library, "Old/song.wav").await;
    let print = heron_analysis::fingerprint_file(&roots.library.join("Old/song.wav"))
        .unwrap()
        .unwrap();
    sqlx::query("INSERT INTO track_fingerprints (track_id, chromaprint) VALUES (?, ?)")
        .bind(existing)
        .bind(print.as_bytes())
        .execute(pool)
        .await
        .unwrap();

    let drop = &roots.incoming;
    std::fs::copy(roots.library.join("Old/song.wav"), drop.join("copy.wav")).unwrap();
    write_tagged(
        &drop.join("retagged.wav"),
        1,
        &Tags {
            title: Some("Another name".into()),
            ..Tags::default()
        },
    );
    write_wav(&drop.join("twin a.wav"), 2);
    std::fs::copy(drop.join("twin a.wav"), drop.join("twin b.wav")).unwrap();
    std::fs::write(drop.join("broken.mp3"), vec![0x5a; 4096]).unwrap();
    // A tenth of a second: too short to fingerprint.
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 11_025,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut short = hound::WavWriter::create(drop.join("short.wav"), spec).unwrap();
    for n in 0..1_100 {
        short.write_sample(((n % 50) * 400) as i16).unwrap();
    }
    short.finalize().unwrap();
    // On an album and a best-of: needs review.
    let album = Album::new(1, 10, false);
    let best_of = Album::new(2, 20, false);
    write_tagged(
        &drop.join("hit.wav"),
        3,
        &Tags {
            title: Some("Hit".into()),
            artist: Some("Someone".into()),
            ..Tags::default()
        },
    );
    write_wav(&drop.join("other.wav"), 4);

    let mut mock = MockState::default();
    mock.acoustid.insert(
        fingerprint(&drop.join("hit.wav")),
        acoustid_result(
            0.97,
            &album.tracks[2].0,
            vec![album.acoustid_group(2), best_of.acoustid_group(5)],
        ),
    );
    mock.acoustid.insert(
        fingerprint(&drop.join("other.wav")),
        acoustid_result(
            0.97,
            &album.tracks[3].0,
            vec![album.acoustid_group(3), best_of.acoustid_group(6)],
        ),
    );
    mock.releases
        .insert(album.release_id.clone(), album.musicbrainz());
    mock.releases
        .insert(best_of.release_id.clone(), best_of.musicbrainz());
    let mock = Mock::start(mock).await;
    sqlx::query("INSERT INTO settings (name, value) VALUES ('library', ?)")
        .bind(
            serde_json::to_string(&LibrarySettings {
                acoustid_key: Some("test-key".into()),
                save_cover: false,
                ..LibrarySettings::default()
            })
            .unwrap(),
        )
        .execute(pool)
        .await
        .unwrap();

    let staged = common::stage(pool, &roots, mock.endpoints(), Duration::ZERO).await;
    assert_eq!(staged.staged, 8);
    assert_eq!(flags(pool, "copy.wav").await, ["duplicate_in_library"]);
    assert_eq!(
        flags(pool, "retagged.wav").await,
        ["duplicate_in_library", "duplicate_in_buffer"],
        "same audio as the library's, and as copy.wav"
    );
    assert!(flags(pool, "twin a.wav").await.is_empty());
    assert_eq!(flags(pool, "twin b.wav").await, ["duplicate_in_buffer"]);
    assert_eq!(flags(pool, "broken.mp3").await.len(), 1);
    assert!(
        ["undecodable", "unsupported_format"]
            .contains(&flags(pool, "broken.mp3").await[0].as_str())
    );
    assert_eq!(flags(pool, "short.wav").await, ["too_short"]);
    let duplicate_of: Option<u64> = sqlx::query_scalar(
        "SELECT duplicate_of_track_id FROM staged_files WHERE original_name = 'copy.wav'",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(duplicate_of, Some(existing));
    assert_eq!(
        planned(pool, "hit.wav").await,
        (
            "Someone/Unknown Album/Hit.wav".to_owned(),
            "ready".to_owned()
        ),
        "its own tags until a release is picked"
    );

    // Pick the album for one, keep the other's own tags (as the API does).
    let choice = serde_json::json!({
        "kind": "release",
        "release_id": album.release_id,
        "recording_id": album.tracks[2].0,
        "disc": 1,
        "track": 3,
    });
    sqlx::query(
        "UPDATE staged_files SET choice = ?, status = 'identifying' WHERE original_name = 'hit.wav'",
    )
    .bind(choice.to_string())
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "UPDATE staged_files SET choice = '{\"kind\": \"own\"}', status = 'identifying'
         WHERE original_name = 'other.wav'",
    )
    .execute(pool)
    .await
    .unwrap();
    let mut stager =
        heron_librarian::staging::Stager::new(pool.clone(), roots.clone(), mock.endpoints());
    stager.identify_once().await.unwrap();
    assert_eq!(
        planned(pool, "hit.wav").await,
        (
            "Band 1/Album 1/03 Song 3 of 1.wav".to_owned(),
            "ready".to_owned()
        )
    );
    assert_eq!(planned(pool, "other.wav").await.1, "ready");

    // Discard the duplicates and the broken files, as the API does, then
    // import the rest.
    sqlx::query(
        "UPDATE staged_files SET status = 'discarded'
         WHERE original_name IN ('copy.wav', 'retagged.wav', 'twin b.wav', 'broken.mp3', 'short.wav')",
    )
    .execute(pool)
    .await
    .unwrap();
    let report = common::import_ready(pool, &roots, mock.endpoints()).await;
    assert_eq!((report.done, report.skipped.len()), (3, 0), "{report:?}");
    assert!(
        roots
            .library
            .join("Band 1/Album 1/03 Song 3 of 1.wav")
            .is_file()
    );
    let statuses: Vec<(String, String)> = sqlx::query_as(
        "SELECT t.path, CAST(i.status AS CHAR) FROM tracks t
         JOIN track_identifications i ON i.track_id = t.id ORDER BY t.path",
    )
    .fetch_all(pool)
    .await
    .unwrap();
    assert_eq!(
        statuses,
        [
            (
                "Band 1/Album 1/03 Song 3 of 1.wav".to_owned(),
                "identified".to_owned()
            ),
            (
                "Unknown Artist/Unknown Album/other.wav".to_owned(),
                "kept".to_owned()
            ),
            (
                "Unknown Artist/Unknown Album/twin a.wav".to_owned(),
                "needs_review".to_owned()
            ),
        ]
    );
    let imported: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM staged_files WHERE status = 'imported' AND track_id IS NOT NULL",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(imported, 3);
    db.drop().await;
}
