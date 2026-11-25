// SPDX-License-Identifier: AGPL-3.0-or-later

//! The library reviewed like the buffer zone: a track searched on
//! MusicBrainz by hand, the tracks never identified identified in the
//! background, and those not on MusicBrainz searched again later.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use common::{Mock, MockState, insert_track, temp_roots, write_wav};
use heron_core::library::{AlbumKind, TaskRequest};
use heron_librarian::identify::Identifier;
use heron_librarian::tasks::TaskRunner;
use heron_service::db::ScratchDatabase;
use serde_json::{Value, json};
use sqlx::MySqlPool;

/// The compilation's release group, given by its address in a search.
const BEST_OF: &str = "5b11f4ce-a62d-471e-81fc-a69a8278c7da";

/// A recording search's `recordings`: the song on its studio album (two
/// editions) and on a compilation, and a live recording.
fn found() -> Value {
    let release = |id: &str, group: &str, title: &str, secondary: &[&str], date: &str| {
        json!({"id": id, "title": title, "status": "Official", "date": date, "country": "GB",
               "release-group": {"id": group, "title": title, "primary-type": "Album",
                                 "secondary-types": secondary},
               "media": [{"position": 1, "track-offset": 2}]})
    };
    json!([
        {"id": "rec", "title": "Hit", "score": 100,
         "artist-credit": [{"name": "Someone", "joinphrase": "", "artist": {"id": "a"}}],
         "releases": [
             release("best", BEST_OF, "Best Of", &["Compilation"], "1990"),
             release("late", "g-album", "Album", &[], "2005"),
             release("first", "g-album", "Album", &[], "1980-05-01"),
         ]},
        {"id": "rec-live", "title": "Hit", "disambiguation": "live", "score": 100,
         "artist-credit": [{"name": "Someone", "joinphrase": "", "artist": {"id": "a"}}],
         "releases": [release("live", "g-live", "Live", &["Live"], "1985")]},
    ])
}

async fn track(pool: &MySqlPool, library: &std::path::Path, name: &str, seed: u64) -> u64 {
    write_wav(&library.join(name), seed);
    let id = insert_track(pool, library, name).await;
    sqlx::query("UPDATE tracks SET title = 'Hit', artist = 'Someone' WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await
        .unwrap();
    id
}

async fn identification(pool: &MySqlPool, track_id: u64) -> (String, Option<String>, Vec<Value>) {
    let (status, reason, candidates): (String, Option<String>, Option<String>) = sqlx::query_as(
        "SELECT CAST(status AS CHAR), reason, CAST(candidates AS CHAR)
         FROM track_identifications WHERE track_id = ?",
    )
    .bind(track_id)
    .fetch_one(pool)
    .await
    .unwrap();
    let candidates = candidates
        .map(|json| serde_json::from_str(&json).unwrap())
        .unwrap_or_default();
    (status, reason, candidates)
}

fn albums(candidates: &[Value]) -> Vec<(&str, &str)> {
    candidates
        .iter()
        .map(|c| {
            (
                c["release_group_title"].as_str().unwrap(),
                c["release_id"].as_str().unwrap(),
            )
        })
        .collect()
}

#[tokio::test]
async fn a_track_searched_by_hand_gets_one_candidate_per_album() {
    let Some(db) = ScratchDatabase::create().await else {
        return;
    };
    let pool = &db.pool;
    let (_dir, roots) = temp_roots();
    let track_id = track(pool, &roots.library, "hit.wav", 1).await;
    let mut state = MockState::default();
    state.recordings.insert("Hit".into(), found());
    let mock = Mock::start(state).await;
    let runner = TaskRunner::new(pool.clone(), roots.clone(), mock.endpoints());

    let search = |kinds| TaskRequest::ReviewSearch {
        track_id,
        title: "Hit (Remastered 2011)".into(),
        artist: "Someone".into(),
        album: None,
        kinds,
    };
    let on_album = |album: String| TaskRequest::ReviewSearch {
        track_id,
        title: "Hit".into(),
        artist: "Someone".into(),
        album: Some(album),
        kinds: Vec::new(),
    };
    common::ask_and_run(&runner, pool, &search(Vec::new())).await;
    let (status, reason, candidates) = identification(pool, track_id).await;
    assert_eq!(status, "needs_review");
    assert!(reason.unwrap().contains("found on MusicBrainz"));
    // Studio albums only, by default, the album's first edition.
    assert_eq!(albums(&candidates), [("Album", "first")]);
    assert_eq!(candidates[0]["album_kind"], "studio");
    assert_eq!(candidates[0]["editions"], 2);
    assert_eq!(candidates[0]["track"], 3);

    common::ask_and_run(
        &runner,
        pool,
        &search(vec![AlbumKind::Live, AlbumKind::Compilation]),
    )
    .await;
    let (_, _, candidates) = identification(pool, track_id).await;
    assert_eq!(albums(&candidates), [("Live", "live"), ("Best Of", "best")]);

    // An album given by its address: that one only, whatever its kind.
    common::ask_and_run(
        &runner,
        pool,
        &on_album(format!("https://musicbrainz.org/release-group/{BEST_OF}")),
    )
    .await;
    let (_, _, candidates) = identification(pool, track_id).await;
    assert_eq!(albums(&candidates), [("Best Of", "best")]);

    // The edition note is left out; the kinds asked are in the query.
    let queries = mock.state.lock().unwrap().recording_queries.clone();
    assert_eq!(
        queries[0],
        r#"recording:"Hit" AND artist:"Someone" AND status:official AND primarytype:(album) AND NOT comment:live"#
    );
    assert!(!queries[1].contains("NOT comment:live"), "{}", queries[1]);
    assert_eq!(queries[2], format!(r#"rgid:{BEST_OF} AND recording:"Hit""#));
    db.drop().await;
}

#[tokio::test]
async fn the_library_is_identified_then_looked_up_again() {
    let Some(db) = ScratchDatabase::create().await else {
        return;
    };
    let pool = &db.pool;
    let (_dir, roots) = temp_roots();
    let never = track(pool, &roots.library, "never.wav", 1).await;
    let kept = track(pool, &roots.library, "kept.wav", 2).await;
    let unknown = track(pool, &roots.library, "unknown.wav", 3).await;
    sqlx::query("UPDATE tracks SET title = 'Nothing' WHERE id = ?")
        .bind(unknown)
        .execute(pool)
        .await
        .unwrap();
    // Kept as not on MusicBrainz, long ago: due to be looked up again.
    for id in [kept, unknown] {
        sqlx::query(
            "INSERT INTO track_identifications (track_id, status, updated_at)
             VALUES (?, 'kept', NOW(6) - INTERVAL 100 DAY)",
        )
        .bind(id)
        .execute(pool)
        .await
        .unwrap();
    }
    let mut state = MockState::default();
    state.recordings.insert("Hit".into(), found());
    let mock = Mock::start(state).await;
    let searcher = Identifier::new(mock.endpoints(), String::new(), 0.9).unwrap();

    // Never identified, without a key: searched by its title and artist.
    let done = heron_librarian::library_review::identify_library(pool, None, &searcher, 10)
        .await
        .unwrap();
    assert_eq!(done, 1);
    let (status, _, candidates) = identification(pool, never).await;
    assert_eq!(status, "needs_review");
    assert_eq!(albums(&candidates), [("Album", "first")]);

    let report = heron_librarian::look_again::look_again(pool, &searcher)
        .await
        .unwrap();
    assert_eq!(report.found, 1);
    let (status, reason, candidates) = identification(pool, kept).await;
    assert_eq!(status, "needs_review");
    assert!(reason.unwrap().contains("on MusicBrainz now"));
    assert_eq!(albums(&candidates), [("Album", "first")]);
    // Not found: it waits 3 months more.
    let (status, _, _) = identification(pool, unknown).await;
    assert_eq!(status, "kept");
    let days: i64 = sqlx::query_scalar(
        "SELECT DATEDIFF(look_again_at, NOW()) FROM track_identifications WHERE track_id = ?",
    )
    .bind(unknown)
    .fetch_one(pool)
    .await
    .unwrap();
    assert!((89..=90).contains(&days), "{days}");
    db.drop().await;
}
