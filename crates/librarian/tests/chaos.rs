// SPDX-License-Identifier: AGPL-3.0-or-later

//! Acceptance (chaos): interrupting the librarian during moves loses no
//! file, overwrites none, and leaves no track pointing at a missing path.
//!
//! The `heron-librarian` binary is crashed at every step of an operation
//! (`HERON_LIBRARIAN_CRASH_AT`, an abort like a power cut) and killed with
//! SIGKILL at random moments, while importing (copies, because identified
//! files are re-tagged; renames for the others), organizing (renames) and
//! undoing. After each crash it is restarted, recovers, and carries on.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::collections::BTreeMap;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use common::{
    Album, Mock, MockState, acoustid_result, fingerprint, temp_roots, write_tagged, write_wav,
};
use heron_core::LibrarySettings;
use heron_librarian::journal::Roots;
use heron_library::{Tags, is_audio, is_hidden};
use heron_service::db::ScratchDatabase;
use sqlx::MySqlPool;

/// The collision target that must never be overwritten.
const PRE_EXISTING: &str = "Band 1/Album 1/01 Song 1 of 1.wav";

struct Setup {
    db_url: String,
    roots: Roots,
    mock_url: String,
}

impl Setup {
    fn command(&self, args: &[&str], crash_at: Option<&str>) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_heron-librarian"));
        command
            .args(args)
            .env("HERON_DATABASE_URL", &self.db_url)
            .env("HERON_LIBRARY_DIR", &self.roots.library)
            .env("HERON_INCOMING_DIR", &self.roots.incoming)
            .env("HERON_HTTP_ADDR", "127.0.0.1:0")
            .env("HERON_IMPORT_SETTLE", "0")
            .env("HERON_INCOMING_INTERVAL", "1")
            .env("HERON_INCOMING_WATCH", "false")
            .env("HERON_ACOUSTID_URL", format!("{}/v2/lookup", self.mock_url))
            .env("HERON_MUSICBRAINZ_URL", format!("{}/ws/2", self.mock_url))
            .env("HERON_COVER_ART_URL", format!("{}/caa", self.mock_url))
            .env("RUST_LOG", "warn")
            .env_remove("HERON_LIBRARIAN_CRASH_AT")
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        if let Some(crash_at) = crash_at {
            command.env("HERON_LIBRARIAN_CRASH_AT", crash_at);
        }
        command
    }

    fn spawn(&self, args: &[&str], crash_at: Option<&str>) -> Child {
        self.command(args, crash_at).spawn().unwrap()
    }

    /// Runs a command to completion (or to its crash).
    fn run(&self, args: &[&str], crash_at: Option<&str>) -> bool {
        let mut command = self.command(args, crash_at);
        let output = command.stderr(Stdio::piped()).output().unwrap();
        eprintln!("chaos: {args:?} crash_at={crash_at:?} -> {}", output.status);
        if crash_at.is_none() && !output.status.success() {
            eprintln!("{}", String::from_utf8_lossy(&output.stderr));
        }
        output.status.success()
    }
}

fn wait(child: &mut Child, timeout: Duration) -> std::process::ExitStatus {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            return status;
        }
        if Instant::now() > deadline {
            child.kill().unwrap();
            panic!("the librarian did not finish in time");
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// Fingerprints of the audio files found in `dirs` (several copies of a
/// file can exist while a copy is in progress).
fn audio(dirs: &[&Path]) -> BTreeMap<String, usize> {
    let mut found = BTreeMap::new();
    for dir in dirs {
        let entries = walkdir::WalkDir::new(dir)
            .into_iter()
            .filter_entry(|entry| entry.depth() == 0 || !is_hidden(entry.file_name()));
        for entry in entries.flatten() {
            if entry.file_type().is_file()
                && is_audio(entry.path())
                && !entry.path().ends_with(PRE_EXISTING)
            {
                *found.entry(fingerprint(entry.path())).or_default() += 1;
            }
        }
    }
    found
}

/// Nothing is lost and nothing is overwritten, whatever the moment.
fn assert_nothing_lost(
    roots: &Roots,
    originals: &BTreeMap<String, usize>,
    pre_existing: &[u8],
    when: &str,
) {
    let found = audio(&[&roots.incoming, &roots.library]);
    for fingerprint in originals.keys() {
        assert!(found.contains_key(fingerprint), "{when}: a file was lost");
    }
    assert_eq!(found.len(), originals.len(), "{when}: unexpected audio");
    assert_eq!(
        std::fs::read(roots.library.join(PRE_EXISTING)).unwrap(),
        pre_existing,
        "{when}: a file was overwritten"
    );
}

async fn assert_settled(
    pool: &MySqlPool,
    roots: &Roots,
    originals: &BTreeMap<String, usize>,
    when: &str,
) {
    let found = audio(&[&roots.incoming, &roots.library]);
    assert_eq!(&found, originals, "{when}: every file exactly once");
    let unfinished: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM library_moves WHERE status IN ('pending', 'moved')",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(unfinished, 0, "{when}: unfinished operations");
    let leftovers: Vec<_> = walkdir::WalkDir::new(&roots.library)
        .into_iter()
        .flatten()
        .filter(|entry| entry.file_name().to_string_lossy().starts_with(".heron-"))
        .map(|entry| entry.path().display().to_string())
        .collect();
    assert!(
        leftovers.is_empty(),
        "{when}: temporary files left: {leftovers:?}"
    );
    common::assert_tracks_match_files(pool, &roots.library).await;
}

fn incoming_is_empty(roots: &Roots) -> bool {
    audio(&[&roots.incoming]).is_empty()
}

#[tokio::test(flavor = "multi_thread")]
async fn interrupted_operations_lose_and_overwrite_nothing() {
    let Some(db) = ScratchDatabase::create().await else {
        return;
    };
    let pool = &db.pool;
    let (_dir, roots) = temp_roots();

    // 4 albums of 4 tracks to identify (re-tagged: copies) and 4 unknown
    // files (renames).
    let mut mock = MockState::default();
    let mut seed = 0;
    for index in 1..=4 {
        let album = Album::new(index, 4, false);
        for (n, (recording, _)) in album.tracks.iter().enumerate() {
            seed += 1;
            let path = roots.incoming.join(format!("drop {index}/{n}.wav"));
            write_tagged(
                &path,
                seed,
                &Tags {
                    title: Some(format!("t{n}")),
                    ..Tags::default()
                },
            );
            mock.acoustid.insert(
                fingerprint(&path),
                acoustid_result(0.99, recording, vec![album.acoustid_group(n)]),
            );
        }
        mock.releases
            .insert(album.release_id.clone(), album.musicbrainz());
    }
    for n in 0..4 {
        seed += 1;
        write_wav(&roots.incoming.join(format!("unknown/{n}.wav")), seed);
    }
    let pre_existing = b"a file already where track 1 of album 1 wants to go".to_vec();
    std::fs::create_dir_all(roots.library.join("Band 1/Album 1")).unwrap();
    std::fs::write(roots.library.join(PRE_EXISTING), &pre_existing).unwrap();
    let originals = audio(&[&roots.incoming]);
    assert_eq!(originals.len(), 20);

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
    let setup = Setup {
        db_url: db.url(),
        roots: roots.clone(),
        mock_url: mock.url.clone(),
    };

    // Import: crash at every step of the first operations, then at random.
    for step in ["recorded", "transferred", "finished", "removed"] {
        for n in [1, 3] {
            if incoming_is_empty(&roots) {
                break;
            }
            let crash_at = format!("{step}:{n}");
            setup.run(&[], Some(&crash_at));
            assert_nothing_lost(
                &roots,
                &originals,
                &pre_existing,
                &format!("import, crash at {crash_at}"),
            );
        }
    }
    let mut round = 0;
    while !incoming_is_empty(&roots) && round < 6 {
        round += 1;
        let mut child = setup.spawn(&[], None);
        std::thread::sleep(Duration::from_millis(300 + 700 * round));
        child.kill().unwrap();
        child.wait().unwrap();
        assert_nothing_lost(
            &roots,
            &originals,
            &pre_existing,
            &format!("import, killed in round {round}"),
        );
    }
    // Then let it finish.
    let mut child = setup.spawn(&[], None);
    let deadline = Instant::now() + Duration::from_secs(120);
    while !incoming_is_empty(&roots) {
        assert!(Instant::now() < deadline, "the import did not finish");
        std::thread::sleep(Duration::from_millis(200));
    }
    // Stop it between operations, as `docker compose stop` does.
    Command::new("kill")
        .arg("-TERM")
        .arg(child.id().to_string())
        .status()
        .unwrap();
    wait(&mut child, Duration::from_secs(60));
    setup.run(&["batches"], None);
    assert_settled(pool, &roots, &originals, "after the import").await;
    assert_eq!(
        std::fs::read(roots.library.join(PRE_EXISTING)).unwrap(),
        pre_existing,
        "the collision got a new name"
    );
    let tracks: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM tracks")
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(tracks, 20, "every file imported once");

    // Organize under another layout, crashing as it goes.
    sqlx::query(
        "UPDATE settings SET value = JSON_SET(value, '$.layout', '{artist}/{title}.{ext}') WHERE name = 'library'",
    )
    .execute(pool)
    .await
    .unwrap();
    let before_organize = common::snapshot(&roots.library);
    for crash_at in ["recorded:1", "transferred:2", "recorded:5", "transferred:7"] {
        setup.run(&["organize", "--apply"], Some(crash_at));
        assert_nothing_lost(
            &roots,
            &originals,
            &pre_existing,
            &format!("organize, crash at {crash_at}"),
        );
    }
    for _ in 0..3 {
        let mut child = setup.spawn(&["organize", "--apply"], None);
        std::thread::sleep(Duration::from_millis(150));
        let _ = child.kill();
        child.wait().unwrap();
        assert_nothing_lost(&roots, &originals, &pre_existing, "organize, killed");
    }
    assert!(setup.run(&["organize", "--apply"], None));
    assert_settled(pool, &roots, &originals, "after organizing").await;
    let organized = common::snapshot(&roots.library);
    assert_ne!(organized, before_organize, "files moved");
    assert!(roots.library.join("Band 2/Song 3 of 2.wav").exists());

    // Undo every organize batch, latest first, crashing as it goes.
    let batches: Vec<u64> = sqlx::query_scalar(
        "SELECT b.id FROM library_batches b WHERE b.kind = 'organize'
           AND EXISTS (SELECT 1 FROM library_moves m WHERE m.batch_id = b.id AND m.status = 'done')
         ORDER BY b.id DESC",
    )
    .fetch_all(pool)
    .await
    .unwrap();
    for (index, batch) in batches.iter().enumerate() {
        let batch = batch.to_string();
        if index == 0 {
            setup.run(&["undo", &batch], Some("transferred:2"));
            assert_nothing_lost(&roots, &originals, &pre_existing, "undo, crashed");
        }
        assert!(setup.run(&["undo", &batch], None), "undo {batch}");
    }
    assert_settled(pool, &roots, &originals, "after undoing").await;
    assert_eq!(
        common::snapshot(&roots.library),
        before_organize,
        "undone byte for byte"
    );
    db.drop().await;
}
