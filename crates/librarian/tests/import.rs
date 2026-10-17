// SPDX-License-Identifier: AGPL-3.0-or-later

//! Acceptance: 100 files (tagged, badly tagged, untagged) dropped into the
//! incoming folder. Confidently identified files end up tagged and filed,
//! the others are imported and flagged for review. AcoustID and MusicBrainz
//! rate limits are respected (against a mock server).

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::collections::HashMap;
use std::time::Duration;

use common::{
    Album, Mock, MockState, Service, acoustid_result, fingerprint, temp_roots, write_tagged,
    write_wav,
};
use heron_core::LibrarySettings;
use heron_librarian::import::Importer;
use heron_library::Tags;
use heron_service::db::ScratchDatabase;
use sqlx::MySqlPool;

async fn set_settings(pool: &MySqlPool, settings: &LibrarySettings) {
    sqlx::query("INSERT INTO settings (name, value) VALUES ('library', ?)")
        .bind(serde_json::to_string(settings).unwrap())
        .execute(pool)
        .await
        .unwrap();
}

fn read_tags(path: &std::path::Path) -> Tags {
    heron_library::tags::read(path).unwrap().tags
}

#[tokio::test]
async fn a_hundred_dropped_files() {
    let Some(db) = ScratchDatabase::create().await else {
        return;
    };
    let pool = &db.pool;
    let (_dir, roots) = temp_roots();
    let incoming = &roots.incoming;
    let mut mock = MockState::default();
    let mut seed = 0;
    let mut next_seed = || {
        seed += 1;
        seed
    };

    // A: 5 well-tagged albums of 8 tracks.
    // B: 3 badly tagged albums of 8 (wrong titles, no artist, no numbers).
    // E: a 16-track various-artists compilation.
    let mut albums = Vec::new();
    for index in 1..=9 {
        let various = index == 9;
        let album = Album::new(index, if various { 16 } else { 8 }, various);
        for (n, (recording, title)) in album.tracks.iter().enumerate() {
            let (folder, tags) = if index <= 5 || various {
                (
                    format!("{}/{:02}.wav", album.title, n + 1),
                    Tags {
                        title: Some(title.clone()),
                        artist: Some(album.track_artist(n + 1)),
                        album: Some(album.title.clone()),
                        track: Some(n as u32 + 1),
                        ..Tags::default()
                    },
                )
            } else {
                (
                    format!("dump {index}/track{:02}.wav", 8 - n),
                    Tags {
                        title: Some(format!("track{:02}", 8 - n)),
                        album: Some("New folder".into()),
                        ..Tags::default()
                    },
                )
            };
            let path = incoming.join(&folder);
            write_tagged(&path, next_seed(), &tags);
            mock.acoustid.insert(
                fingerprint(&path),
                acoustid_result(0.98, recording, vec![album.acoustid_group(n)]),
            );
        }
        mock.releases
            .insert(album.release_id.clone(), album.musicbrainz());
        albums.push(album);
    }
    mock.covers.insert(
        albums[0].release_id.clone(),
        b"\xff\xd8\xff\xe0 cover of album 1".to_vec(),
    );

    // C: 10 untagged files AcoustID does not know.
    for n in 1..=10 {
        write_wav(&incoming.join(format!("unknown/rec{n}.wav")), next_seed());
    }
    // F: 6 lone files, each on an album and a best-of, nothing to choose with.
    for n in 1..=6 {
        let album = Album::new(100 + n, 10, false);
        let best_of = Album::new(200 + n, 20, false);
        let path = incoming.join(format!("singles/{n}/single.wav"));
        write_tagged(
            &path,
            next_seed(),
            &Tags {
                title: Some(format!("Hit {n}")),
                ..Tags::default()
            },
        );
        mock.acoustid.insert(
            fingerprint(&path),
            acoustid_result(
                0.97,
                &album.tracks[2].0,
                vec![album.acoustid_group(2), best_of.acoustid_group(5)],
            ),
        );
    }
    // D: 4 files matched with a low score.
    let doubtful = Album::new(300, 4, false);
    for n in 0..4 {
        let path = incoming.join(format!("doubtful/{n}.wav"));
        write_wav(&path, next_seed());
        mock.acoustid.insert(
            fingerprint(&path),
            acoustid_result(0.5, &doubtful.tracks[n].0, vec![doubtful.acoustid_group(n)]),
        );
    }
    mock.releases
        .insert(doubtful.release_id.clone(), doubtful.musicbrainz());
    assert_eq!(mock.acoustid.len(), 90, "every fingerprint is distinct");

    let mock = Mock::start(mock).await;
    set_settings(
        pool,
        &LibrarySettings {
            acoustid_key: Some("test-key".into()),
            ..LibrarySettings::default()
        },
    )
    .await;

    let mut importer = Importer::new(
        pool.clone(),
        roots.clone(),
        mock.endpoints(),
        Duration::ZERO,
    );
    let report = importer.import_once().await.unwrap();
    assert_eq!(
        (
            report.imported,
            report.identified,
            report.needs_review,
            report.failed,
            report.deferred
        ),
        (100, 80, 20, 0, 0),
        "{report:?}"
    );

    // Everything was taken from the incoming folder, folders included.
    assert!(common::snapshot(incoming).is_empty());
    assert_eq!(std::fs::read_dir(incoming).unwrap().count(), 0);
    common::assert_tracks_match_files(pool, &roots.library).await;

    // Well-tagged albums: filed and tagged from MusicBrainz.
    let library = &roots.library;
    let filed = library.join("Band 1/Album 1/03 Song 3 of 1.wav");
    let tags = read_tags(&filed);
    assert_eq!(tags.album_artist.as_deref(), Some("Band 1"));
    assert_eq!(tags.date.as_deref(), Some("1991"), "the original year");
    assert_eq!(
        (tags.track, tags.track_total, tags.disc, tags.disc_total),
        (Some(3), Some(8), Some(1), Some(1))
    );
    assert_eq!(
        tags.musicbrainz.release.as_deref(),
        Some(albums[0].release_id.as_str())
    );
    assert_eq!(
        tags.musicbrainz.recording.as_deref(),
        Some(albums[0].tracks[2].0.as_str())
    );
    assert_eq!(
        std::fs::read(library.join("Band 1/Album 1/cover.jpg")).unwrap(),
        b"\xff\xd8\xff\xe0 cover of album 1"
    );
    assert!(
        !heron_library::tags::read(&filed).unwrap().has_cover,
        "not embedded by default"
    );

    // Badly tagged albums: fixed by their siblings.
    let fixed = read_tags(&library.join("Band 7/Album 7/01 Song 1 of 7.wav"));
    assert_eq!(fixed.title.as_deref(), Some("Song 1 of 7"));
    assert_eq!(fixed.artist.as_deref(), Some("Band 7"));

    // The compilation.
    let compilation = read_tags(&library.join("Compilations/Album 9/12 Song 12 of 9.wav"));
    assert_eq!(compilation.artist.as_deref(), Some("Artist 12"));
    assert!(compilation.compilation);

    // The others keep their own tags.
    assert!(
        library
            .join("Unknown Artist/Unknown Album/rec7.wav")
            .exists()
    );
    assert!(
        library
            .join("Unknown Artist/Unknown Album/Hit 1.wav")
            .exists()
    );
    let reasons: Vec<(String, i64)> = sqlx::query_as(
        "SELECT reason, COUNT(*) FROM track_identifications WHERE status = 'needs_review'
         GROUP BY reason ORDER BY reason",
    )
    .fetch_all(pool)
    .await
    .unwrap();
    assert_eq!(
        reasons,
        [
            ("best AcoustID score 0.50 is below 0.90".to_owned(), 4),
            ("matches 2 releases, none clearly better".to_owned(), 6),
            ("not found on AcoustID".to_owned(), 10),
        ]
    );
    let counts: (i64, i64, i64) = sqlx::query_as(
        "SELECT (SELECT COUNT(*) FROM track_fingerprints), (SELECT COUNT(*) FROM track_tag_changes),
                (SELECT COUNT(*) FROM analysis_jobs)",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(
        counts,
        (100, 80, 100),
        "fingerprints, tag changes, analysis jobs"
    );

    // Rate limits: one MusicBrainz request per second, three for AcoustID.
    let musicbrainz = mock.requests(Service::MusicBrainz);
    assert_eq!(
        musicbrainz.len(),
        9,
        "one request per release, not per file"
    );
    for pair in musicbrainz.windows(2) {
        assert!(
            pair[1] - pair[0] >= Duration::from_millis(990),
            "{:?}",
            pair[1] - pair[0]
        );
    }
    let acoustid = mock.requests(Service::AcoustId);
    assert_eq!(acoustid.len(), 100);
    for window in acoustid.windows(4) {
        assert!(
            window[3] - window[0] >= Duration::from_millis(990),
            "{:?}",
            window[3] - window[0]
        );
    }
    let agents = mock.state.lock().unwrap().user_agents.clone();
    assert!(
        agents
            .iter()
            .all(|agent| agent.starts_with("HeronBroadcaster/")
                && agent.contains("heronbroadcaster.org"))
    );

    // Re-tagging keeps the analysis: undo the import's tag changes.
    let track_id: u64 = sqlx::query_scalar(
        "SELECT id FROM tracks WHERE path = 'Band 1/Album 1/03 Song 3 of 1.wav'",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO track_analysis (track_id, analysis_version, params_hash, cue_in_ms, cue_out_ms)
         VALUES (?, 2, '', 0, 8000)",
    )
    .bind(track_id)
    .execute(pool)
    .await
    .unwrap();
    let batch: u64 = sqlx::query_scalar("SELECT id FROM library_batches WHERE kind = 'import'")
        .fetch_one(pool)
        .await
        .unwrap();
    let undone = heron_librarian::organize::undo(pool, &roots, batch)
        .await
        .unwrap();
    assert_eq!((undone.done, undone.skipped.len()), (80, 0), "{undone:?}");
    let restored = read_tags(&filed);
    assert_eq!(restored.musicbrainz.release, None);
    assert_eq!(restored.title.as_deref(), Some("Song 3 of 1"));
    assert_eq!(restored.track_total, None);
    let analyses: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM track_analysis WHERE track_id = ?")
            .bind(track_id)
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!(analyses, 1, "the analysis was kept");
    common::assert_tracks_match_files(pool, library).await;
    db.drop().await;
}

#[tokio::test]
async fn without_a_key_files_are_filed_by_their_own_tags() {
    let Some(db) = ScratchDatabase::create().await else {
        return;
    };
    let pool = &db.pool;
    let (_dir, roots) = temp_roots();
    let tags = Tags {
        title: Some("Song".into()),
        artist: Some("Band".into()),
        album: Some("Album".into()),
        track: Some(4),
        ..Tags::default()
    };
    write_tagged(&roots.incoming.join("drop/a.wav"), 1, &tags);
    write_wav(&roots.incoming.join("b.wav"), 2);
    // Already in the library under the name a.wav would get.
    write_wav(&roots.library.join("Band/Album/04 Song.wav"), 3);
    let existing = std::fs::read(roots.library.join("Band/Album/04 Song.wav")).unwrap();
    let original = std::fs::read(roots.incoming.join("drop/a.wav")).unwrap();
    // Not audio, and hidden: left alone.
    std::fs::write(roots.incoming.join("drop/notes.txt"), b"x").unwrap();
    write_wav(&roots.incoming.join(".hidden.wav"), 4);

    let mock = Mock::start(MockState::default()).await;
    let mut importer = Importer::new(
        pool.clone(),
        roots.clone(),
        mock.endpoints(),
        Duration::ZERO,
    );
    let report = importer.import_once().await.unwrap();
    assert_eq!(
        (report.imported, report.identified, report.needs_review),
        (2, 0, 0),
        "{report:?}"
    );
    assert!(mock.state.lock().unwrap().requests.is_empty());

    let library = &roots.library;
    assert_eq!(
        std::fs::read(library.join("Band/Album/04 Song.wav")).unwrap(),
        existing,
        "never overwritten"
    );
    assert_eq!(
        std::fs::read(library.join("Band/Album/04 Song (2).wav")).unwrap(),
        original,
        "moved byte for byte"
    );
    assert!(library.join("Unknown Artist/Unknown Album/b.wav").exists());
    assert!(roots.incoming.join("drop/notes.txt").exists());
    assert!(roots.incoming.join(".hidden.wav").exists());
    let identifications: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM track_identifications")
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(identifications, 0, "nothing flagged for review");
    let titles: HashMap<String, Option<String>> =
        sqlx::query_as::<_, (String, Option<String>)>("SELECT path, title FROM tracks")
            .fetch_all(pool)
            .await
            .unwrap()
            .into_iter()
            .collect();
    assert_eq!(
        titles["Band/Album/04 Song (2).wav"].as_deref(),
        Some("Song")
    );
    db.drop().await;
}

#[tokio::test]
async fn files_still_being_copied_wait() {
    let Some(db) = ScratchDatabase::create().await else {
        return;
    };
    let (_dir, roots) = temp_roots();
    write_wav(&roots.incoming.join("fresh.wav"), 1);
    let mock = Mock::start(MockState::default()).await;
    let mut importer = Importer::new(
        db.pool.clone(),
        roots.clone(),
        mock.endpoints(),
        Duration::from_secs(60),
    );
    let report = importer.import_once().await.unwrap();
    assert_eq!((report.imported, report.deferred), (0, 1));
    assert!(roots.incoming.join("fresh.wav").exists());
    db.drop().await;
}
