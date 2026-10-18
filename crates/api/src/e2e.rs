// SPDX-License-Identifier: AGPL-3.0-or-later

//! End to end: the API over HTTP and WebSocket, a scratch MariaDB database
//! (`HERON_TEST_DATABASE_URL`) and a real engine playing a small library in
//! real time, in process. Skipped without a test database.

use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use heron_engine::config::EngineConfig;
use heron_engine::engine::{Engine, EngineOptions};
use heron_engine::grpc::EngineService;
use heron_service::db::{DatabaseHealth, ScratchDatabase};
use reqwest::{Method, StatusCode};
use serde_json::{Value, json};
use tempfile::TempDir;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::TcpListenerStream;
use tokio_tungstenite::tungstenite;

use crate::app::{self, AppState};
use crate::auth::{Passwords, Role};
use crate::library::Library;
use crate::{engine, history, queue, sync, users};

/// Tracks short enough to see several play during a test.
const TRACK_SECONDS: f64 = 3.0;

struct Station {
    db: ScratchDatabase,
    library: TempDir,
    _state_dir: TempDir,
    _engine: Arc<Engine>,
    state: AppState,
    api: SocketAddr,
    tracks: Vec<u64>,
    tasks: Vec<tokio::task::JoinHandle<()>>,
}

fn write_wav(path: &Path, seconds: f64, freq: f64) {
    let spec = hound::WavSpec {
        channels: 2,
        sample_rate: 44_100,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(path, spec).unwrap();
    for n in 0..(seconds * 44_100.0) as u64 {
        let t = n as f64 / 44_100.0;
        let value = (t * freq * std::f64::consts::TAU).sin() * 0.3 * f64::from(i16::MAX);
        writer.write_sample(value as i16).unwrap();
        writer.write_sample(value as i16).unwrap();
    }
    writer.finalize().unwrap();
}

impl Station {
    async fn start() -> Option<Self> {
        Self::start_with(None).await
    }

    /// With the engine's configuration saved before it starts, as `ctl` left it.
    async fn start_with(engine_config: Option<EngineConfig>) -> Option<Self> {
        let db = ScratchDatabase::create().await?;
        let pool = db.pool.clone();

        // A library of analyzed tracks, as the scanner and analyzer leave it.
        let library = tempfile::tempdir().unwrap();
        let mut tracks = Vec::new();
        for n in 1..=5 {
            let path = format!("Artist {n}/Track {n}.wav");
            std::fs::create_dir_all(library.path().join(format!("Artist {n}"))).unwrap();
            write_wav(
                &library.path().join(&path),
                TRACK_SECONDS,
                220.0 * f64::from(n),
            );
            let id = sqlx::query(
                "INSERT INTO tracks (path, title, artist, album, duration_ms) VALUES (?, ?, ?, ?, ?)",
            )
            .bind(&path)
            .bind(format!("Track {n}"))
            .bind(format!("Artist {n}"))
            .bind("Album")
            .bind(3_000u32)
            .execute(&pool)
            .await
            .unwrap()
            .last_insert_id();
            sqlx::query(
                "INSERT INTO track_analysis (track_id, analysis_version, params_hash, cue_in_ms, cue_out_ms,
                                             integrated_lufs, true_peak_db)
                 VALUES (?, 1, ?, 0, 3000, -13.0, -7.0)",
            )
            .bind(id)
            .bind([0u8; 32].as_slice())
            .execute(&pool)
            .await
            .unwrap();
            tracks.push(id);
        }

        let state_dir = tempfile::tempdir().unwrap();
        if let Some(config) = engine_config {
            config.save(state_dir.path()).unwrap();
        }
        let engine = Engine::start(
            EngineOptions {
                library_dir: library.path().to_path_buf(),
                state_dir: state_dir.path().to_path_buf(),
                monitor: false,
            },
            tokio::runtime::Handle::current(),
        )
        .unwrap();
        let grpc = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let grpc_addr = grpc.local_addr().unwrap();
        let service = EngineService::server(Arc::clone(&engine));
        let mut tasks = vec![tokio::spawn(async move {
            let _ = tonic::transport::Server::builder()
                .add_service(service)
                .serve_with_incoming(TcpListenerStream::new(grpc))
                .await;
        })];

        let link = engine::EngineLink::new(&format!("http://{grpc_addr}")).unwrap();
        let queue = queue::QueueManager::new(pool.clone(), Arc::clone(&link));
        let sync = sync::ConfigSync::new(pool.clone(), Arc::clone(&link));
        let history_events = link.events();
        tasks.extend(link.spawn());
        tasks.push(tokio::spawn(
            history::HistoryRecorder::new(pool.clone()).run(history_events),
        ));
        tasks.push(tokio::spawn(Arc::clone(&queue).run()));
        tasks.push(tokio::spawn(Arc::clone(&sync).run()));

        let state = AppState {
            pool: pool.clone(),
            health: DatabaseHealth(pool.clone()),
            library: Arc::new(Library::new(pool.clone(), library.path().to_path_buf())),
            engine: link,
            queue,
            sync,
            passwords: Arc::new(Passwords::default()),
        };
        let http = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let api = http.local_addr().unwrap();
        let router = app::router(state.clone());
        tasks.push(tokio::spawn(async move {
            let _ = axum::serve(http, router).await;
        }));

        for (name, role) in [("admin", Role::Admin), ("viewer", Role::Viewer)] {
            users::add_user(&pool, &state.passwords, name, "correct horse", role)
                .await
                .unwrap();
        }
        Some(Self {
            db,
            library,
            _state_dir: state_dir,
            _engine: engine,
            state,
            api,
            tracks,
            tasks,
        })
    }

    fn client(&self) -> Client {
        Client {
            http: reqwest::Client::new(),
            base: format!("http://{}/api/v1", self.api),
            cookie: None,
            bearer: None,
        }
    }

    async fn signed_in(&self, username: &str) -> Client {
        let mut client = self.client();
        let response = client
            .http
            .post(format!("{}/auth/login", client.base))
            .json(&json!({"username": username, "password": "correct horse"}))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let cookie = response.headers()["set-cookie"]
            .to_str()
            .unwrap()
            .to_owned();
        assert!(cookie.contains("HttpOnly"));
        client.cookie = Some(cookie.split(';').next().unwrap().to_owned());
        client
    }

    async fn stop(self) {
        for task in self.tasks {
            task.abort();
        }
        self.db.drop().await;
    }
}

struct Client {
    http: reqwest::Client,
    base: String,
    cookie: Option<String>,
    bearer: Option<String>,
}

impl Client {
    async fn request(
        &self,
        method: Method,
        path: &str,
        body: Option<Value>,
    ) -> (StatusCode, Value) {
        let mut request = self.http.request(method, format!("{}{path}", self.base));
        if let Some(cookie) = &self.cookie {
            request = request
                .header("cookie", cookie)
                .header("x-requested-with", "test");
        }
        if let Some(token) = &self.bearer {
            request = request.bearer_auth(token);
        }
        if let Some(body) = body {
            request = request.json(&body);
        }
        let response = request.send().await.unwrap();
        let status = response.status();
        let text = response.text().await.unwrap();
        let value = serde_json::from_str(&text).unwrap_or(Value::Null);
        (status, value)
    }

    async fn get(&self, path: &str) -> Value {
        let (status, value) = self.request(Method::GET, path, None).await;
        assert_eq!(status, StatusCode::OK, "GET {path}: {value}");
        value
    }

    async fn send(&self, method: Method, path: &str, body: Value) -> (StatusCode, Value) {
        self.request(method, path, Some(body)).await
    }
}

/// Polls `check` until it returns something, for at most `timeout`.
async fn eventually<T, F: Future<Output = Option<T>>>(
    what: &str,
    timeout: Duration,
    mut check: impl FnMut() -> F,
) -> T {
    let start = Instant::now();
    loop {
        if let Some(value) = check().await {
            return value;
        }
        assert!(start.elapsed() < timeout, "timed out waiting for {what}");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

fn waiting_tracks(queue: &Value) -> Vec<u64> {
    queue
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["track"]["track_id"].as_u64().unwrap())
        .collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn sign_in_and_roles() {
    let Some(station) = Station::start().await else {
        return;
    };
    let anonymous = station.client();
    // Public.
    let status = anonymous.get("/status").await;
    assert_eq!(status["database"], "ok");
    assert_eq!(anonymous.get("/library/status").await["tracks"], 5);
    let openapi = anonymous.get("/openapi.json").await;
    assert!(openapi["paths"]["/api/v1/queue"]["post"].is_object());
    assert!(openapi["components"]["schemas"]["LiveMessage"].is_object());

    // Everything else needs a session or a token.
    let (status, body) = anonymous.request(Method::GET, "/queue", None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["error"], "unauthorized");
    let (status, _) = anonymous
        .send(
            Method::POST,
            "/auth/login",
            json!({"username": "admin", "password": "wrong password"}),
        )
        .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let viewer = station.signed_in("viewer").await;
    assert_eq!(viewer.get("/auth/me").await["role"], "viewer");
    let (status, _) = viewer.request(Method::POST, "/player/skip", None).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _) = viewer.request(Method::GET, "/users", None).await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // A cookie alone cannot change anything (cross-site requests).
    let admin = station.signed_in("admin").await;
    let response = admin
        .http
        .post(format!("{}/player/skip", admin.base))
        .header("cookie", admin.cookie.as_deref().unwrap())
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    // API tokens act as their user.
    let (status, token) = admin
        .send(Method::POST, "/tokens", json!({"name": "automation"}))
        .await;
    assert_eq!(status, StatusCode::CREATED);
    let mut scripted = station.client();
    scripted.bearer = Some(token["token"].as_str().unwrap().to_owned());
    assert_eq!(scripted.get("/auth/me").await["username"], "admin");
    let (status, _) = admin
        .request(Method::DELETE, &format!("/tokens/{}", token["id"]), None)
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, _) = scripted.request(Method::GET, "/auth/me", None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    // The last admin stays an admin.
    let me = admin.get("/auth/me").await;
    let (status, _) = admin
        .send(
            Method::PATCH,
            &format!("/users/{}", me["id"]),
            json!({"role": "viewer"}),
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT);

    // Signing out ends the session.
    let (status, _) = admin.request(Method::POST, "/auth/logout", None).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, _) = admin.request(Method::GET, "/auth/me", None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    station.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn queue_manager_feeds_the_engine_and_records_history() {
    let Some(station) = Station::start().await else {
        return;
    };
    let admin = station.signed_in("admin").await;

    // Autofill keeps three tracks waiting, and something goes on air.
    let queue = eventually("autofill", Duration::from_secs(10), || async {
        let queue = admin.get("/queue").await;
        (queue.as_array().unwrap().len() >= 3).then_some(queue)
    })
    .await;
    assert!(
        queue.as_array().unwrap()[0]["track"]["source"] == "rotation",
        "{queue}"
    );
    let player = eventually("a track on air", Duration::from_secs(10), || async {
        let player = admin.get("/player").await;
        let on_air = player["decks"]
            .as_array()
            .unwrap()
            .iter()
            .any(|deck| deck["on_air"] == true);
        on_air.then_some(player)
    })
    .await;
    assert_eq!(player["paused"], false);

    // A track queued next, then moved and removed.
    let manual = station.tracks[4];
    let (status, queued) = admin
        .send(
            Method::POST,
            "/queue",
            json!({"track_ids": [manual], "position": 0}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{queued}");
    let entry = queued["entry_ids"][0].as_u64().unwrap();
    let waiting = admin.get("/queue").await;
    assert_eq!(waiting[0]["entry_id"], entry);
    assert_eq!(waiting[0]["track"]["source"], "manual");
    let (status, _) = admin
        .send(
            Method::PUT,
            &format!("/queue/{entry}/position"),
            json!({"position": 2}),
        )
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let waiting = admin.get("/queue").await;
    assert_eq!(waiting[2]["entry_id"], entry, "{waiting}");
    let (status, _) = admin
        .request(Method::DELETE, &format!("/queue/{entry}"), None)
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert!(!waiting_tracks(&admin.get("/queue").await).is_empty());

    // Missing files are refused.
    sqlx::query("UPDATE tracks SET missing_since = NOW(6) WHERE id = ?")
        .bind(manual)
        .execute(&station.db.pool)
        .await
        .unwrap();
    let (status, body) = admin
        .send(Method::POST, "/queue", json!({"track_ids": [manual]}))
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");

    // The live socket reports track changes.
    let url = format!("ws://{}/api/v1/live?levels=false", station.api);
    let mut request = tungstenite::client::IntoClientRequest::into_client_request(url).unwrap();
    request
        .headers_mut()
        .insert("cookie", admin.cookie.as_deref().unwrap().parse().unwrap());
    let (mut socket, _) = tokio_tungstenite::connect_async(request).await.unwrap();
    let mut next = async || -> Value {
        let message = tokio::time::timeout(Duration::from_secs(10), socket.next())
            .await
            .expect("live message")
            .unwrap()
            .unwrap();
        serde_json::from_str(message.to_text().unwrap()).unwrap()
    };
    let first = next().await;
    assert_eq!(first["type"], "state");
    assert!(first["player"]["decks"].is_array());

    // Skip: the play ends as skipped in the history, the next one starts.
    let (status, _) = admin.request(Method::POST, "/player/skip", None).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let mut saw_event = false;
    for _ in 0..50 {
        let message = next().await;
        if message["type"] == "track" {
            saw_event = true;
            break;
        }
    }
    assert!(saw_event, "no track event on the live socket");
    let history = eventually("a skipped play", Duration::from_secs(10), || async {
        let history = admin.get("/history").await;
        let skipped = history
            .as_array()
            .unwrap()
            .iter()
            .any(|play| play["end_reason"] == "skipped");
        skipped.then_some(history)
    })
    .await;
    let play = &history[0];
    assert!(
        play["track_id"].is_u64() && play["artist"].is_string(),
        "{play}"
    );
    assert_eq!(play["source"], "rotation");

    // Every play that ended is complete.
    eventually("finished plays", Duration::from_secs(10), || async {
        let history = admin.get("/history").await;
        history
            .as_array()
            .unwrap()
            .iter()
            .any(|play| {
                play["end_reason"] == "finished" && play["played_ms"].as_u64() > Some(2_000)
            })
            .then_some(())
    })
    .await;

    // Autofill off: the queue drains.
    let (status, _) = admin
        .send(
            Method::PUT,
            "/settings/queue",
            json!({"autofill": false, "min_queued": 3}),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = admin.request(Method::DELETE, "/queue", None).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    tokio::time::sleep(Duration::from_secs(3)).await;
    assert!(waiting_tracks(&admin.get("/queue").await).is_empty());
    station.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn settings_and_outputs_reach_the_engine() {
    let Some(station) = Station::start().await else {
        return;
    };
    let admin = station.signed_in("admin").await;
    eventually("the engine", Duration::from_secs(10), || async {
        let engine = admin.get("/engine").await;
        (engine["config"]["state"] == "synced").then_some(())
    })
    .await;

    // Playout settings.
    let mut playout = admin.get("/settings/playout").await;
    playout["crossfade"]["min_track_ms"] = json!(30_000);
    let (status, body) = admin.send(Method::PUT, "/settings/playout", playout).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    eventually(
        "playout settings in the engine",
        Duration::from_secs(5),
        || async {
            let config = station.state.engine.config().await.ok()?;
            config
                .playout_settings_json
                .contains("\"min_track_ms\":30000")
                .then_some(())
        },
    )
    .await;

    // An output: validated, stored without showing its password, pushed.
    let profiles = admin.get("/encoder-profiles").await;
    let profile = profiles[0]["id"].as_u64().unwrap();
    let (status, body) = admin
        .send(
            Method::POST,
            "/outputs",
            json!({"name": "Bad", "kind": "icecast", "encoder_profile_id": profile,
                   "config": {"host": "127.0.0.1", "mount": "live", "password": "x"}}),
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    let (status, output) = admin
        .send(
            Method::POST,
            "/outputs",
            json!({"name": "Icecast", "kind": "icecast", "encoder_profile_id": profile,
                   "config": {"host": "127.0.0.1", "port": 9, "mount": "/live", "password": "hackme"}}),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{output}");
    assert!(output["config"].get("password").is_none());
    assert_eq!(output["secrets_set"], json!(["password"]));
    let id = output["id"].as_u64().unwrap();
    eventually(
        "the output in the engine",
        Duration::from_secs(10),
        || async {
            let config = station.state.engine.config().await.ok()?;
            config.outputs_json.contains("hackme").then_some(())
        },
    )
    .await;

    // Updating without the password keeps it.
    let (status, _) = admin
        .send(
            Method::PUT,
            &format!("/outputs/{id}"),
            json!({"name": "Icecast", "kind": "icecast", "encoder_profile_id": profile,
                   "config": {"host": "127.0.0.1", "port": 9, "mount": "/other"}}),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    eventually(
        "the changed output in the engine",
        Duration::from_secs(10),
        || async {
            let config = station.state.engine.config().await.ok()?;
            (config.outputs_json.contains("/other") && config.outputs_json.contains("hackme"))
                .then_some(())
        },
    )
    .await;
    let outputs = eventually("the output's state", Duration::from_secs(10), || async {
        let outputs = admin.get("/outputs").await;
        outputs[0]["status"].is_object().then_some(outputs)
    })
    .await;
    assert_ne!(outputs[0]["status"]["state"], "connected");

    // A profile in use cannot be deleted.
    let (status, _) = admin
        .request(
            Method::DELETE,
            &format!("/encoder-profiles/{profile}"),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT);
    let (status, _) = admin
        .request(Method::DELETE, &format!("/outputs/{id}"), None)
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    eventually(
        "the output gone from the engine",
        Duration::from_secs(10),
        || async {
            let config = station.state.engine.config().await.ok()?;
            (!config.outputs_json.contains("hackme")).then_some(())
        },
    )
    .await;

    // Library settings never show the AcoustID key.
    let (status, library) = admin
        .send(
            Method::PATCH,
            "/settings/library",
            json!({"acoustid_key": "s3cret"}),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(library["acoustid_key_set"], true);
    assert!(!library.to_string().contains("s3cret"));
    let (status, _) = admin
        .send(
            Method::PATCH,
            "/settings/library",
            json!({"layout": "{nope}"}),
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    station.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn library_tracks_overrides_and_categories() {
    let Some(station) = Station::start().await else {
        return;
    };
    let admin = station.signed_in("admin").await;
    let page = admin.get("/tracks?q=artist%203&limit=10").await;
    assert_eq!(page["total"], 1);
    let id = page["items"][0]["id"].as_u64().unwrap();
    assert_eq!(admin.get("/tracks?filter=playable").await["total"], 5);

    let (status, category) = admin
        .send(
            Method::POST,
            "/categories",
            json!({"name": "Gold", "color": "#FFAA00"}),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    let (status, duplicate) = admin
        .send(Method::POST, "/categories", json!({"name": "Gold"}))
        .await;
    assert_eq!(status, StatusCode::CONFLICT, "{duplicate}");
    let (status, assigned) = admin
        .send(
            Method::PUT,
            &format!("/tracks/{id}/categories"),
            json!([{"category_id": category["id"], "weight": 2.0}]),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{assigned}");
    let in_gold = admin
        .get(&format!("/tracks?category_id={}", category["id"]))
        .await;
    assert_eq!(in_gold["total"], 1);

    let (status, body) = admin
        .send(
            Method::PUT,
            &format!("/tracks/{id}/overrides"),
            json!({"cue_in_ms": 500, "crossfade": {"crossfade": false}}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let detail = admin.get(&format!("/tracks/{id}")).await;
    assert_eq!(detail["playout"]["cue_in_ms"], 500);
    assert_eq!(detail["overrides"]["crossfade"]["crossfade"], false);
    assert_eq!(detail["categories"][0]["name"], "Gold");
    assert_eq!(detail["analysis"]["cue_out_ms"], 3000);
    // Loudness: −13 LUFS brought to −16.
    assert_eq!(detail["playout"]["gain_db"], -3.0);
    let (status, _) = admin
        .send(
            Method::PUT,
            &format!("/tracks/{id}/overrides"),
            json!({"cue_in_ms": 4000, "cue_out_ms": 1000}),
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);

    let (status, preview) = admin
        .send(
            Method::POST,
            "/playout/transition",
            json!({"from_track_id": station.tracks[0], "to_track_id": id}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{preview}");
    // Tracks under 20 s play back to back.
    assert_eq!(preview["overlap_ms"], 0);

    let (status, _) = admin
        .send(
            Method::PATCH,
            &format!("/tracks/{id}"),
            json!({"enabled": false}),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(admin.get("/tracks?filter=disabled").await["total"], 1);
    station.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn queued_tracks_follow_the_library() {
    let Some(station) = Station::start().await else {
        return;
    };
    let admin = station.signed_in("admin").await;
    let (status, _) = admin
        .send(
            Method::PUT,
            "/settings/queue",
            json!({"autofill": true, "min_queued": 4}),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let queue = eventually("four waiting tracks", Duration::from_secs(10), || async {
        let queue = admin.get("/queue").await;
        (queue.as_array().unwrap().len() >= 4).then_some(queue)
    })
    .await;

    // The librarian moves the last waiting track's file, and records it.
    let last = &queue[3];
    let (entry, track) = (
        last["entry_id"].as_u64().unwrap(),
        last["track"]["track_id"].as_u64().unwrap(),
    );
    let old = last["track"]["path"].as_str().unwrap().to_owned();
    let new = format!("Moved/{}", old.replace('/', " - "));
    std::fs::create_dir_all(station.library.path().join("Moved")).unwrap();
    std::fs::rename(
        station.library.path().join(&old),
        station.library.path().join(&new),
    )
    .unwrap();
    sqlx::query("UPDATE tracks SET path = ? WHERE id = ?")
        .bind(&new)
        .bind(track)
        .execute(&station.db.pool)
        .await
        .unwrap();
    eventually(
        "the queued track to follow its file",
        Duration::from_secs(8),
        || async {
            let queue = admin.get("/queue").await;
            queue
                .as_array()
                .unwrap()
                .iter()
                .any(|e| e["entry_id"] == entry && e["track"]["path"] == new.as_str())
                .then_some(())
        },
    )
    .await;

    // A waiting track whose file disappeared leaves the queue.
    let queue = admin.get("/queue").await;
    let gone = &queue[queue.as_array().unwrap().len() - 1];
    let gone_entry = gone["entry_id"].as_u64().unwrap();
    sqlx::query("UPDATE tracks SET missing_since = NOW(6) WHERE id = ?")
        .bind(gone["track"]["track_id"].as_u64().unwrap())
        .execute(&station.db.pool)
        .await
        .unwrap();
    eventually(
        "the missing track out of the queue",
        Duration::from_secs(8),
        || async {
            let queue = admin.get("/queue").await;
            (!queue
                .as_array()
                .unwrap()
                .iter()
                .any(|e| e["entry_id"] == gone_entry))
            .then_some(())
        },
    )
    .await;
    station.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn adopts_outputs_configured_on_the_engine() {
    let engine_config: EngineConfig = serde_json::from_value(json!({
        "playout": {"crossfade": {"min_track_ms": 25000}},
        "outputs": {
            "encoder_profiles": [
                {"id": 1, "name": "MP3 128k", "encoder": "mp3", "bitrate_kbps": 128,
                 "sample_rate": 44100, "channels": 2},
                {"id": 7, "name": "MP3 64k", "encoder": "mp3", "bitrate_kbps": 64,
                 "sample_rate": 44100, "channels": 2}
            ],
            "outputs": [
                {"id": 3, "name": "Low", "kind": "icecast", "encoder_profile_id": 7,
                 "config": {"host": "127.0.0.1", "port": 9, "mount": "/low", "password": "p"}}
            ]
        }
    }))
    .unwrap();
    let Some(station) = Station::start_with(Some(engine_config.clone())).await else {
        return;
    };
    let admin = station.signed_in("admin").await;
    eventually("the first sync", Duration::from_secs(10), || async {
        let engine = admin.get("/engine").await;
        (engine["config"]["state"] == "synced").then_some(())
    })
    .await;
    let outputs = admin.get("/outputs").await;
    assert_eq!(outputs[0]["id"], 3);
    assert_eq!(outputs[0]["encoder_profile_id"], 7);
    let playout = admin.get("/settings/playout").await;
    assert_eq!(playout["crossfade"]["min_track_ms"], 25000);

    // Same IDs: the engine kept its configuration as it was.
    let config = station.state.engine.config().await.unwrap();
    let outputs: heron_core::OutputsConfig = serde_json::from_str(&config.outputs_json).unwrap();
    assert_eq!(outputs.outputs, engine_config.outputs.outputs);
    station.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn web_ui_routes() {
    let Some(station) = Station::start().await else {
        return;
    };
    let pool = &station.state.pool;
    let admin = station.signed_in("admin").await;
    let viewer = station.signed_in("viewer").await;
    let track = station.tracks[0];

    // The theme each user picked.
    assert_eq!(admin.get("/auth/me").await["theme"], "system");
    let (status, me) = admin
        .send(Method::PUT, "/auth/preferences", json!({"theme": "dark"}))
        .await;
    assert_eq!(status, StatusCode::OK, "{me}");
    assert_eq!(me["theme"], "dark");
    assert_eq!(admin.get("/auth/me").await["theme"], "dark");
    assert_eq!(viewer.get("/auth/me").await["theme"], "system");

    // The server clock, for the dashboard.
    let status = station.client().get("/status").await;
    assert!(status["clock"].is_object());
    assert!(status["server_time_unix_ms"].as_i64().unwrap() > 0);

    // Waveforms come with the analysis.
    let (status, _) = viewer
        .request(Method::GET, &format!("/tracks/{track}/waveform"), None)
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    sqlx::query("INSERT INTO track_waveforms (track_id, interval_ms, peaks) VALUES (?, 50, ?)")
        .bind(track)
        .bind([0u8, 128, 255].as_slice())
        .execute(pool)
        .await
        .unwrap();
    let waveform = viewer.get(&format!("/tracks/{track}/waveform")).await;
    assert_eq!(waveform["interval_ms"], 50);
    assert_eq!(waveform["peaks"], "AID/");

    // Audio for previews, with range requests.
    let audio = format!("{}/tracks/{track}/audio", viewer.base);
    let response = viewer
        .http
        .get(&audio)
        .header("cookie", viewer.cookie.as_deref().unwrap())
        .header("range", "bytes=0-3")
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::PARTIAL_CONTENT);
    assert_eq!(&response.bytes().await.unwrap()[..], b"RIFF");
    let response = reqwest::get(&audio).await.unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    // Organizing goes through heron-librarian, after a dry run.
    let roots = heron_librarian::journal::Roots {
        library: station.library.path().to_path_buf(),
        incoming: station.library.path().join(".incoming"),
    };
    let librarian = heron_librarian::tasks::TaskRunner::new(
        pool.clone(),
        roots,
        heron_librarian::identify::Endpoints::default(),
    );
    let (status, _) = viewer
        .request(Method::POST, "/library/organize", None)
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, plan) = admin.request(Method::POST, "/library/organize", None).await;
    assert_eq!(status, StatusCode::ACCEPTED, "{plan}");
    assert_eq!(plan["status"], "pending");
    assert_eq!(plan["request"]["kind"], "organize_plan");
    let plan_id = plan["id"].as_u64().unwrap();
    let apply = format!("/library/organize/{plan_id}/apply");
    let (status, _) = admin.request(Method::POST, &apply, None).await;
    assert_eq!(status, StatusCode::CONFLICT, "not finished yet");
    assert!(librarian.run_next().await.unwrap());
    let plan = admin.get(&format!("/library/tasks/{plan_id}")).await;
    assert_eq!(plan["status"], "done", "{plan}");
    assert_eq!(plan["plan"]["moves"].as_array().unwrap().len(), 5);
    assert_eq!(
        plan["plan"]["moves"][0]["to"],
        "Unknown Artist/Unknown Album/Track 1.wav"
    );
    let (status, applying) = admin.request(Method::POST, &apply, None).await;
    assert_eq!(status, StatusCode::ACCEPTED, "{applying}");
    let (status, _) = admin.request(Method::POST, &apply, None).await;
    assert_eq!(status, StatusCode::CONFLICT, "applied once");
    assert!(librarian.run_next().await.unwrap());
    let applied = admin
        .get(&format!("/library/tasks/{}", applying["id"]))
        .await;
    assert_eq!(applied["report"]["done"], 5, "{applied}");
    assert!(
        station
            .library
            .path()
            .join("Unknown Artist/Unknown Album/Track 1.wav")
            .exists()
    );
    let batches = viewer.get("/library/batches").await;
    assert_eq!(batches[0]["kind"], "organize");
    assert_eq!(batches[0]["files"], 5);
    let batch = batches[0]["id"].as_u64().unwrap();
    let (status, undo) = admin
        .request(
            Method::POST,
            &format!("/library/batches/{batch}/undo"),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::ACCEPTED, "{undo}");
    assert!(librarian.run_next().await.unwrap());
    assert!(station.library.path().join("Artist 1/Track 1.wav").exists());
    let (status, _) = admin
        .request(
            Method::POST,
            &format!("/library/batches/{batch}/undo"),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT, "undone once");
    assert_eq!(
        viewer.get("/library/tasks").await.as_array().unwrap().len(),
        3
    );

    // Review: a candidate release, or the file's own tags.
    let candidates = json!([{"score": 0.7, "recording_id": "rec", "release_id": "rel",
                             "release_title": "Album", "disc": 1, "track": 1}]);
    sqlx::query(
        "INSERT INTO track_identifications (track_id, status, reason, score, candidates)
         VALUES (?, 'needs_review', 'doubtful', 0.7, ?)",
    )
    .bind(track)
    .bind(candidates.to_string())
    .execute(pool)
    .await
    .unwrap();
    let review = viewer.get("/library/review").await;
    assert_eq!(review["total"], 1);
    assert_eq!(review["items"][0]["track"]["id"], track);
    assert_eq!(review["items"][0]["candidates"][0]["release_id"], "rel");
    let (status, _) = admin
        .send(
            Method::POST,
            &format!("/tracks/{track}/review/apply"),
            json!({"release_id": "other", "recording_id": "rec"}),
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    let (status, picked) = admin
        .send(
            Method::POST,
            &format!("/tracks/{track}/review/apply"),
            json!({"release_id": "rel", "recording_id": "rec"}),
        )
        .await;
    assert_eq!(status, StatusCode::ACCEPTED, "{picked}");
    assert_eq!(picked["request"]["track"], 1);
    let (status, _) = admin
        .send(
            Method::POST,
            &format!("/tracks/{track}/review/apply"),
            json!({"release_id": "rel", "recording_id": "rec"}),
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT, "already being applied");
    let keep = format!("/tracks/{track}/review/keep");
    let (status, _) = viewer.request(Method::POST, &keep, None).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _) = admin.request(Method::POST, &keep, None).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, _) = admin.request(Method::POST, &keep, None).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(viewer.get("/library/review").await["total"], 0);
    station.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn rotation_follows_clocks_restrictions_and_logs_its_picks() {
    let Some(station) = Station::start().await else {
        return;
    };
    let admin = station.signed_in("admin").await;
    let viewer = station.signed_in("viewer").await;
    let pool = &station.db.pool;
    let t = &station.tracks;

    crate::artists::link_pending(pool).await.unwrap();
    assert_eq!(admin.get("/artists?q=artist%201").await["total"], 1);

    // Artists from the tags: names matched loosely, featuring split.
    for (track, artist) in [
        (t[0], "Neal Morse"),
        (t[1], "NÉAL  MORSE feat. Mike Portnoy"),
        (t[2], "The Neal Morse Band"),
        (t[3], "Simon & Garfunkel"),
        (t[4], "Paul Simon"),
    ] {
        sqlx::query("UPDATE tracks SET artist = ? WHERE id = ?")
            .bind(artist)
            .bind(track)
            .execute(pool)
            .await
            .unwrap();
    }
    sqlx::query("UPDATE tracks SET artist = 'Simon' WHERE id = ?")
        .bind(t[4])
        .execute(pool)
        .await
        .unwrap();
    crate::artists::link_pending(pool).await.unwrap();
    let page = admin.get("/artists?q=neal%20morse").await;
    assert_eq!(page["total"], 2, "{page}");
    let neal = page["artists"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["name"] == "Neal Morse")
        .unwrap()
        .clone();
    assert_eq!(neal["tracks"], 2, "{neal}");
    assert_eq!(neal["names"].as_array().unwrap().len(), 1);
    let band = page["artists"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["name"] == "The Neal Morse Band")
        .unwrap()
        .clone();
    assert_eq!(admin.get("/artists?q=portnoy").await["total"], 1);
    // Linking again changes nothing; the artists of the old tags are gone.
    assert_eq!(crate::artists::link_pending(pool).await.unwrap(), 0);
    assert_eq!(admin.get("/artists?q=artist%201").await["total"], 0);

    // "Simon & Garfunkel" names Simon: suggested, never applied alone.
    let suggestions = admin.get("/artist-suggestions").await;
    let suggestion = &suggestions.as_array().unwrap()[0];
    assert_eq!(suggestion["source"], "names", "{suggestions}");
    let (status, _) = viewer
        .send(
            Method::POST,
            &format!("/artist-suggestions/{}", suggestion["id"]),
            json!({"accept": true}),
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _) = admin
        .send(
            Method::POST,
            &format!("/artist-suggestions/{}", suggestion["id"]),
            json!({"accept": true}),
        )
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert!(
        admin
            .get("/artist-suggestions")
            .await
            .as_array()
            .unwrap()
            .is_empty()
    );
    let simon = &admin.get("/artists?q=garfunkel").await["artists"][0];
    assert_eq!(simon["similar"].as_array().unwrap().len(), 1, "{simon}");

    // The band is the same artist: merged, its tracks follow.
    let (status, merged) = admin
        .send(
            Method::POST,
            &format!("/artists/{}/merge", neal["id"]),
            json!({"artist_id": band["id"]}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{merged}");
    assert_eq!(merged["tracks"], 3);
    assert_eq!(merged["names"].as_array().unwrap().len(), 2);
    // A name moved back to an artist of its own.
    let band_name = merged["names"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["name"] == "The Neal Morse Band")
        .unwrap()["id"]
        .clone();
    let (status, own) = admin
        .send(
            Method::PUT,
            &format!("/artist-names/{band_name}"),
            json!({}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{own}");
    assert_eq!(own["tracks"], 1);
    let (status, _) = admin
        .request(
            Method::PUT,
            &format!("/artists/{}/similar/{}", neal["id"], own["id"]),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    // Settings: the time zone and the rotation rules are checked.
    let (status, _) = admin
        .send(
            Method::PUT,
            "/settings/station",
            json!({"timezone": "Mars/Olympus"}),
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    let (status, _) = admin
        .send(
            Method::PUT,
            "/settings/station",
            json!({"timezone": "Europe/Paris"}),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = admin
        .send(
            Method::PUT,
            "/settings/rotation",
            json!({"weights": {"play_factor": 0.1}}),
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);

    // Two categories, a clock calling them in turn, all week.
    let mut categories = Vec::new();
    for (name, members) in [("A", &t[..3]), ("B", &t[3..])] {
        let (status, category) = admin
            .send(
                Method::POST,
                "/categories",
                json!({"name": name, "separation": {"track_min": 1}}),
            )
            .await;
        assert_eq!(status, StatusCode::CREATED, "{category}");
        assert_eq!(category["separation"]["track_min"], 1);
        for track in members {
            let (status, _) = admin
                .send(
                    Method::PUT,
                    &format!("/tracks/{track}/categories"),
                    json!([{"category_id": category["id"]}]),
                )
                .await;
            assert_eq!(status, StatusCode::OK);
        }
        categories.push(category["id"].as_u64().unwrap());
    }
    let (status, _) = viewer
        .send(
            Method::POST,
            "/clocks",
            json!({"name": "Nope", "slots": []}),
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, clock) = admin
        .send(
            Method::POST,
            "/clocks",
            json!({"name": "Hour", "slots": [
                {"category_id": categories[0], "method": "random"},
                {"category_id": categories[1], "method": "weighted",
                 "separation": {"artist_min": 0}},
            ]}),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{clock}");
    let clock_id = clock["id"].as_u64().unwrap();
    assert_eq!(clock["slots"][1]["separation"]["artist_min"], 0);
    assert_eq!(clock["looping"], false);
    let week: Vec<Value> = (1..=7)
        .flat_map(|day| {
            (0..24).map(move |hour| json!({"day_of_week": day, "hour": hour, "clock_id": clock_id}))
        })
        .collect();
    let (status, schedule) = admin.send(Method::PUT, "/schedule", json!(week)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(schedule.as_array().unwrap().len(), 168);
    let (status, _) = admin
        .send(
            Method::PUT,
            "/schedule",
            json!([{"day_of_week": 8, "hour": 0, "clock_id": clock_id}]),
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    let (status, _) = admin
        .send(
            Method::DELETE,
            &format!("/categories/{}", categories[0]),
            json!({}),
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT, "clocks use it");

    // The last track is banned all week: never picked.
    let banned = t[4];
    let (status, restriction) = admin
        .send(
            Method::POST,
            "/restrictions",
            json!({
                "target": {"kind": "track", "id": banned},
                "rule": {"kind": "daypart", "mode": "ban", "days": [1, 2, 3, 4, 5, 6, 7],
                         "start_hour": 0, "end_hour": 24},
                "note": "pulled",
            }),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{restriction}");
    assert!(
        restriction["target_name"]
            .as_str()
            .unwrap()
            .contains("Simon")
    );
    let (status, _) = admin
        .send(
            Method::POST,
            "/restrictions",
            json!({
                "target": {"kind": "category", "id": categories[0]},
                "rule": {"kind": "daypart", "mode": "ban", "days": [], "start_hour": 0, "end_hour": 24},
            }),
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        admin
            .get(&format!("/restrictions?target=track&target_id={banned}"))
            .await
            .as_array()
            .unwrap()
            .len(),
        1
    );

    // Weights halve with each play.
    let (status, _) = admin
        .send(
            Method::PUT,
            "/settings/rotation",
            json!({"weights": {"play_factor": 0.5}}),
        )
        .await;
    assert_eq!(status, StatusCode::OK);

    // From now on the clock runs: A, B, A, B…
    let (status, _) = admin.request(Method::DELETE, "/queue", None).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let first_pick: u64 =
        sqlx::query_scalar("SELECT CAST(COALESCE(MAX(id), 0) AS UNSIGNED) FROM rotation_picks")
            .fetch_one(pool)
            .await
            .unwrap();
    let picks = eventually(
        "clock picks that played",
        Duration::from_secs(20),
        || async {
            let picks = admin.get("/rotation/picks?limit=200").await;
            let picks: Vec<Value> = picks
                .as_array()
                .unwrap()
                .iter()
                .filter(|p| p["id"].as_u64().unwrap() > first_pick)
                .cloned()
                .collect();
            let played = picks
                .iter()
                .filter(|p| p["played_at_unix_ms"].is_i64())
                .count();
            (picks.len() >= 6 && played >= 3).then_some(picks)
        },
    )
    .await;
    let mut slots: Vec<(u64, u64)> = picks
        .iter()
        .rev()
        .map(|p| {
            let explanation = &p["explanation"];
            assert_eq!(explanation["clock_id"], clock_id, "{p}");
            assert_ne!(p["track_id"], banned, "{p}");
            (
                explanation["slot"].as_u64().unwrap(),
                explanation["slot_category_id"].as_u64().unwrap(),
            )
        })
        .collect();
    slots.dedup();
    assert!(slots.len() >= 4, "slots alternate: {slots:?}");
    for (slot, category) in &slots {
        assert_eq!(*category, categories[*slot as usize]);
    }
    // Tracks repeat within a minute: rules were relaxed, and the plays say so.
    let relaxed: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM play_history h JOIN rotation_picks p ON p.play_id = h.id
         WHERE p.step > 0 AND JSON_VALUE(h.relaxation_applied, '$.step') = p.step",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    let relaxed_picks: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM rotation_picks WHERE step > 0 AND play_id IS NOT NULL",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    assert!(relaxed_picks > 0);
    assert_eq!(relaxed, relaxed_picks);
    let weight: f64 =
        sqlx::query_scalar("SELECT MIN(weight) FROM track_categories WHERE category_id = ?")
            .bind(categories[0])
            .fetch_one(pool)
            .await
            .unwrap();
    assert!(weight < 1.0, "weights moved: {weight}");

    // The clock as it will sound, and a simulation of it.
    let (status, preview) = admin
        .send(
            Method::POST,
            "/clocks/preview",
            json!({"slots": clock["slots"]}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{preview}");
    assert_eq!(preview["slots"][0]["tracks"], 3, "{preview}");
    assert_eq!(preview["slots"][1]["average_ms"], 3_000);
    assert!(preview["runs_per_hour"].as_f64().unwrap() > 100.0);
    // A looping clock: nothing falls past the hour.
    let long: Vec<Value> = (0..30)
        .map(|_| json!({"category_id": categories[0]}))
        .collect();
    let (status, looped) = admin
        .send(
            Method::POST,
            "/clocks/preview",
            json!({"slots": long, "looping": true}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{looped}");
    assert!(
        looped["slots"]
            .as_array()
            .unwrap()
            .iter()
            .all(|s| s["beyond_hour"] == false)
    );
    let (status, saved) = admin
        .send(
            Method::PUT,
            &format!("/clocks/{clock_id}"),
            json!({"name": "Hour", "looping": true, "slots": clock["slots"]}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{saved}");
    assert_eq!(saved["looping"], true);
    let (status, report) = admin
        .send(
            Method::POST,
            "/rotation/simulate",
            json!({"hours": 2, "seed": 7}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{report}");
    let simulation = &report["simulation"];
    assert!(
        simulation["violations"].as_array().unwrap().is_empty(),
        "{simulation}"
    );
    let simulated = simulation["picks"].as_array().unwrap();
    assert!(simulated.len() > 2_000);
    assert!(simulated.iter().all(|p| p["track_id"] != banned));
    assert_eq!(report["tracks"].as_array().unwrap().len(), 4);
    let (status, _) = viewer
        .send(Method::POST, "/rotation/simulate", json!({"hours": 1}))
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    station.stop().await;
}
