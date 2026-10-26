// SPDX-License-Identifier: AGPL-3.0-or-later

//! Two Icecast outputs at different bitrates, fed from the same master bus:
//! both stream, and losing one server does not disturb the other.

// Test helpers outside `#[test]` functions.
#![allow(clippy::unwrap_used)]

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use heron_core::{EncoderKind, EncoderProfile, OutputConfig, OutputKind, OutputsConfig};
use heron_outputs::{NowPlaying, OutputState, Outputs};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::watch;

/// A fake Icecast server: accepts sources and metadata updates, counts the
/// audio bytes it receives and remembers the request lines.
struct FakeIcecast {
    port: u16,
    bytes: Arc<AtomicU64>,
    requests: Arc<Mutex<Vec<String>>>,
    task: tokio::task::JoinHandle<()>,
}

impl FakeIcecast {
    async fn start(port: u16) -> Self {
        let listener = TcpListener::bind(("127.0.0.1", port)).await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let bytes = Arc::new(AtomicU64::new(0));
        let requests = Arc::new(Mutex::new(Vec::new()));
        let (task_bytes, task_requests) = (Arc::clone(&bytes), Arc::clone(&requests));
        let task = tokio::spawn(async move {
            let mut connections = tokio::task::JoinSet::new();
            loop {
                let Ok((mut socket, _)) = listener.accept().await else {
                    break;
                };
                let (bytes, requests) = (Arc::clone(&task_bytes), Arc::clone(&task_requests));
                connections.spawn(async move {
                    let mut header = Vec::new();
                    let mut byte = [0_u8; 1];
                    while !header.ends_with(b"\r\n\r\n") {
                        if socket.read(&mut byte).await.unwrap_or(0) == 0 {
                            return;
                        }
                        header.push(byte[0]);
                    }
                    let text = String::from_utf8_lossy(&header).into_owned();
                    let line = text.lines().next().unwrap_or_default().to_owned();
                    requests.lock().unwrap().push(line.clone());
                    if line.starts_with("GET /admin/metadata") {
                        let _ = socket.write_all(b"HTTP/1.0 200 OK\r\n\r\n").await;
                        return;
                    }
                    let _ = socket.write_all(b"HTTP/1.1 100 Continue\r\n\r\n").await;
                    let mut buffer = [0_u8; 8_192];
                    while let Ok(read) = socket.read(&mut buffer).await {
                        if read == 0 {
                            break;
                        }
                        bytes.fetch_add(read as u64, Ordering::Relaxed);
                    }
                });
            }
        });
        Self {
            port,
            bytes,
            requests,
            task,
        }
    }

    fn received(&self) -> u64 {
        self.bytes.load(Ordering::Relaxed)
    }

    /// Stops the server and drops every connection.
    fn kill(self) -> u16 {
        self.task.abort();
        self.port
    }
}

fn profile(id: u64, bitrate: u16, sample_rate: u32) -> EncoderProfile {
    EncoderProfile {
        id,
        name: format!("MP3 {bitrate}k"),
        encoder: EncoderKind::Mp3,
        bitrate_kbps: Some(bitrate),
        sample_rate,
        channels: 2,
        options: serde_json::Value::Null,
    }
}

fn output(id: u64, profile: u64, port: u16) -> OutputConfig {
    OutputConfig {
        id,
        name: format!("icecast-{id}"),
        kind: OutputKind::Icecast,
        encoder_profile_id: profile,
        config: serde_json::json!({
            "host": "127.0.0.1",
            "port": port,
            "mount": "/live",
            "password": "hackme",
        }),
        metadata_enabled: true,
        enabled: true,
    }
}

/// Feeds the master bus in real time, like the audio thread.
fn feed(mut master: rtrb::Producer<f32>, stop: Arc<AtomicBool>) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let block: Vec<f32> = (0..960).map(|n| ((n as f32) * 0.05).sin() * 0.3).collect();
        let start = Instant::now();
        let mut blocks = 0_u32;
        while !stop.load(Ordering::Relaxed) {
            if let Ok(mut chunk) = master.write_chunk(block.len()) {
                let (first, second) = chunk.as_mut_slices();
                let split = first.len();
                first.copy_from_slice(&block[..split]);
                second.copy_from_slice(&block[split..]);
                chunk.commit_all();
            }
            blocks += 1;
            let due = start + Duration::from_millis(10) * blocks;
            std::thread::sleep(due.saturating_duration_since(Instant::now()));
        }
    })
}

async fn wait_until(what: &str, mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !condition() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn one_server_going_down_does_not_affect_the_other() {
    let server_a = FakeIcecast::start(0).await;
    let server_b = FakeIcecast::start(0).await;
    let (master_tx, master_rx) = rtrb::RingBuffer::new(48_000 * 2 * 2);
    let stop = Arc::new(AtomicBool::new(false));
    let feeder = feed(master_tx, Arc::clone(&stop));
    let (metadata_tx, metadata_rx) = watch::channel(NowPlaying::default());

    let mut outputs =
        Outputs::new(master_rx, metadata_rx, tokio::runtime::Handle::current()).unwrap();
    outputs
        .apply(&OutputsConfig {
            encoder_profiles: vec![profile(1, 128, 44_100), profile(2, 64, 22_050)],
            outputs: vec![output(1, 1, server_a.port), output(2, 2, server_b.port)],
        })
        .unwrap();

    wait_until("both outputs to stream", || {
        server_a.received() > 16_000 && server_b.received() > 8_000
    })
    .await;

    // Metadata goes to both servers.
    metadata_tx.send_replace(NowPlaying {
        artist: Some("Artist".into()),
        title: Some("Title".into()),
    });
    wait_until("metadata", || {
        [&server_a, &server_b].iter().all(|server| {
            server
                .requests
                .lock()
                .unwrap()
                .iter()
                .any(|line| line.contains("song=Artist%20%2D%20Title"))
        })
    })
    .await;

    // Server A goes away; B keeps receiving 64 kbps (8,000 bytes/s).
    let port_a = server_a.kill();
    let before = server_b.received();
    let started = Instant::now();
    tokio::time::sleep(Duration::from_secs(2)).await;
    let rate = (server_b.received() - before) as f64 / started.elapsed().as_secs_f64();
    assert!(
        (7_000.0..9_000.0).contains(&rate),
        "output B received {rate:.0} bytes/s"
    );
    let snapshot = outputs.snapshot();
    assert!(
        matches!(snapshot[0].state, Some(OutputState::Reconnecting { .. })),
        "{snapshot:?}"
    );
    assert!(matches!(
        snapshot[1].state,
        Some(OutputState::Connected { .. })
    ));

    // A comes back and its output reconnects on its own.
    let server_a = FakeIcecast::start(port_a).await;
    wait_until("output A to reconnect", || server_a.received() > 16_000).await;

    stop.store(true, Ordering::Relaxed);
    feeder.join().unwrap();
}

#[tokio::test]
async fn changing_one_output_leaves_the_others_streaming() {
    let server_a = FakeIcecast::start(0).await;
    let server_b = FakeIcecast::start(0).await;
    let (master_tx, master_rx) = rtrb::RingBuffer::new(48_000 * 2 * 2);
    let stop = Arc::new(AtomicBool::new(false));
    let feeder = feed(master_tx, Arc::clone(&stop));
    let (_metadata_tx, metadata_rx) = watch::channel(NowPlaying::default());
    let mut outputs =
        Outputs::new(master_rx, metadata_rx, tokio::runtime::Handle::current()).unwrap();

    let mut config = OutputsConfig {
        encoder_profiles: vec![profile(1, 128, 44_100)],
        outputs: vec![output(1, 1, server_a.port), output(2, 1, server_b.port)],
    };
    outputs.apply(&config).unwrap();
    wait_until("both outputs to stream", || {
        server_a.received() > 0 && server_b.received() > 0
    })
    .await;
    let sources = |server: &FakeIcecast| {
        server
            .requests
            .lock()
            .unwrap()
            .iter()
            .filter(|line| line.starts_with("PUT"))
            .count()
    };

    // Disabling B does not reconnect A.
    config.outputs[1].enabled = false;
    outputs.apply(&config).unwrap();
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(sources(&server_a), 1);
    assert_eq!(outputs.snapshot()[1].state, None);

    // An invalid configuration is refused as a whole.
    let mut invalid = config.clone();
    invalid.outputs[0].config = serde_json::json!({"host": "127.0.0.1"});
    assert!(outputs.apply(&invalid).is_err());
    assert_eq!(outputs.config(), &config);

    stop.store(true, Ordering::Relaxed);
    feeder.join().unwrap();
}

fn external(id: u64, codec: &str) -> EncoderProfile {
    EncoderProfile {
        id,
        name: format!("FFmpeg {codec}"),
        encoder: EncoderKind::ExternalFfmpeg,
        bitrate_kbps: Some(96),
        sample_rate: 44_100,
        channels: 2,
        options: serde_json::json!({ "codec": codec }),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_external_ffmpeg_output_streams_and_a_broken_one_reports_why() {
    let has_ffmpeg = std::process::Command::new("ffmpeg")
        .arg("-version")
        .output()
        .is_ok_and(|output| output.status.success());
    if !has_ffmpeg {
        eprintln!("ffmpeg not installed, skipping");
        return;
    }
    let (mp3, aac, broken) = (
        FakeIcecast::start(0).await,
        FakeIcecast::start(0).await,
        FakeIcecast::start(0).await,
    );
    let (master_tx, master_rx) = rtrb::RingBuffer::new(48_000 * 2 * 2);
    let stop = Arc::new(AtomicBool::new(false));
    let feeder = feed(master_tx, Arc::clone(&stop));
    let (_metadata_tx, metadata_rx) = watch::channel(NowPlaying::default());
    let mut outputs =
        Outputs::new(master_rx, metadata_rx, tokio::runtime::Handle::current()).unwrap();
    outputs
        .apply(&OutputsConfig {
            encoder_profiles: vec![
                profile(1, 128, 44_100),
                external(2, "aac"),
                external(3, "no_such_codec"),
            ],
            outputs: vec![
                output(1, 1, mp3.port),
                output(2, 2, aac.port),
                output(3, 3, broken.port),
            ],
        })
        .unwrap();

    wait_until("the MP3 and AAC outputs to stream", || {
        mp3.received() > 16_000 && aac.received() > 12_000
    })
    .await;
    wait_until("the broken encoder's error", || {
        outputs.snapshot()[2]
            .encoder_error
            .as_deref()
            .is_some_and(|error| error.contains("no_such_codec"))
    })
    .await;
    let snapshot = outputs.snapshot();
    assert_eq!(snapshot[0].encoder_error, None);
    assert_eq!(snapshot[1].encoder_error, None);
    assert_eq!(broken.received(), 0);

    stop.store(true, Ordering::Relaxed);
    feeder.join().unwrap();
}
