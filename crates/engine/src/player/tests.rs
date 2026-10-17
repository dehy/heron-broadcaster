// SPDX-License-Identifier: AGPL-3.0-or-later

//! The player driving a mixer rendered offline, with real files.

use std::time::Duration;

use heron_core::playout::CrossfadeOverrides;
use tempfile::TempDir;

use super::*;
use crate::mixer::{CHANNELS, Mixer};

const BLOCK: usize = 4_800;

struct Station {
    library: TempDir,
    player: Player,
    mixer: Mixer,
    events: broadcast::Receiver<PlayerEvent>,
    now_playing: watch::Receiver<NowPlaying>,
}

impl Station {
    fn new() -> Self {
        let library = tempfile::tempdir().unwrap();
        let (mixer, handle) = Mixer::new(BLOCK);
        let (events_tx, events) = broadcast::channel(64);
        let (now_playing_tx, now_playing) = watch::channel(NowPlaying::default());
        let player = Player::new(
            library.path().to_path_buf(),
            PlayoutSettings::default(),
            handle,
            events_tx,
            now_playing_tx,
        );
        Self {
            library,
            player,
            mixer,
            events,
            now_playing,
        }
    }

    /// Writes a 44.1 kHz stereo WAV of `seconds` of a tone into the library.
    fn file(&self, name: &str, seconds: f64) -> TrackRequest {
        let spec = hound::WavSpec {
            channels: 2,
            sample_rate: 44_100,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = hound::WavWriter::create(self.library.path().join(name), spec).unwrap();
        for n in 0..(seconds * 44_100.0) as u64 {
            let value = (n as f64 * 0.06).sin() * 0.3 * f64::from(i16::MAX);
            writer.write_sample(value as i16).unwrap();
            writer.write_sample(value as i16).unwrap();
        }
        writer.finalize().unwrap();
        let duration_ms = (seconds * 1_000.0) as u32;
        TrackRequest {
            track_id: None,
            path: name.to_owned(),
            title: Some(name.to_owned()),
            artist: Some("Tester".to_owned()),
            album: None,
            playout: TrackPlayout::unanalyzed(duration_ms),
        }
    }

    /// Renders `seconds` of audio at ten times real time, letting the
    /// decoders keep up.
    fn play(&mut self, seconds: f64) -> Vec<f32> {
        let mut output = Vec::new();
        let mut block = vec![0.0; BLOCK * CHANNELS];
        for _ in 0..(seconds * 10.0).round() as usize {
            self.player.tick();
            std::thread::sleep(Duration::from_millis(10));
            self.mixer.render(&mut block);
            output.extend_from_slice(&block);
        }
        self.player.tick();
        output
    }

    fn events(&mut self) -> Vec<PlayerEvent> {
        std::iter::from_fn(|| self.events.try_recv().ok()).collect()
    }
}

fn started(events: &[PlayerEvent]) -> Vec<(Deck, String)> {
    events
        .iter()
        .filter_map(|event| match event {
            PlayerEvent::Started { deck, entry, .. } => Some((*deck, entry.track.path.clone())),
            PlayerEvent::Ended { .. } => None,
        })
        .collect()
}

#[test]
fn plays_the_queue_in_order_on_alternating_decks() {
    let mut station = Station::new();
    let tracks = vec![
        station.file("one.wav", 1.0),
        station.file("two.wav", 1.0),
        station.file("three.wav", 1.0),
    ];
    station.player.enqueue(tracks, None).unwrap();
    station.play(3.5);

    let events = station.events();
    assert_eq!(
        started(&events),
        [
            (Deck::A, "one.wav".to_owned()),
            (Deck::B, "two.wav".to_owned()),
            (Deck::A, "three.wav".to_owned()),
        ]
    );
    let ended = events
        .iter()
        .filter(|event| {
            matches!(
                event,
                PlayerEvent::Ended {
                    played_ms: 1_000,
                    reason: EndReason::Finished,
                    ..
                }
            )
        })
        .count();
    assert_eq!(ended, 3);
    assert_eq!(station.player.snapshot().underruns, 0);
    assert_eq!(
        station.now_playing.borrow().title.as_deref(),
        Some("three.wav")
    );
}

#[test]
fn crossfades_at_the_mix_point() {
    let mut station = Station::new();
    let mut first = station.file("first.wav", 24.0);
    first.playout.mix_point_ms = Some(22_000);
    let second = station.file("second.wav", 24.0);
    station.player.enqueue(vec![first, second], None).unwrap();
    station.play(23.0);

    let snapshot = station.player.snapshot();
    let (a, b) = (&snapshot.decks[0], &snapshot.decks[1]);
    // The second track started at 22 s, 1 s ago, on Deck B; the first fades out.
    assert_eq!(b.position_ms, 1_000);
    assert!(b.on_air && !a.on_air);
    assert_eq!(a.position_ms, 23_000);
    assert_eq!(
        started(&station.events()),
        [
            (Deck::A, "first.wav".to_owned()),
            (Deck::B, "second.wav".to_owned()),
        ]
    );
}

#[test]
fn per_track_overrides_disable_the_crossfade() {
    let mut station = Station::new();
    let mut first = station.file("first.wav", 24.0);
    first.playout.mix_point_ms = Some(22_000);
    first.playout.crossfade = CrossfadeOverrides {
        crossfade: Some(false),
        ..CrossfadeOverrides::default()
    };
    let second = station.file("second.wav", 24.0);
    station.player.enqueue(vec![first, second], None).unwrap();
    station.play(23.0);
    let snapshot = station.player.snapshot();
    assert_eq!(snapshot.decks[1].entry, None);
    assert!(snapshot.decks[0].on_air);
}

#[test]
fn applies_the_loudness_gain() {
    let mut station = Station::new();
    let mut track = station.file("loud.wav", 1.0);
    track.playout.integrated_lufs = Some(-10.0);
    station.player.enqueue(vec![track], None).unwrap();
    let audio = station.play(0.5);
    // -16 LUFS target: 6 dB down from a 0.3 sine.
    let peak = audio.iter().fold(0.0_f32, |peak, s| peak.max(s.abs()));
    assert!((peak - 0.3 * 0.501).abs() < 0.01, "peak {peak}");
}

#[test]
fn skip_starts_the_next_track() {
    let mut station = Station::new();
    let tracks = vec![station.file("one.wav", 5.0), station.file("two.wav", 5.0)];
    station.player.enqueue(tracks, None).unwrap();
    station.play(1.0);
    station.player.skip().unwrap();
    station.play(1.5);
    let events = station.events();
    assert!(events.iter().any(|event| matches!(
        event,
        PlayerEvent::Ended {
            reason: EndReason::Skipped,
            played_ms: 2_000,
            ..
        }
    )));
    assert_eq!(started(&events).len(), 2);
}

#[test]
fn queue_changes_reach_the_cued_track() {
    let mut station = Station::new();
    let tracks = vec![station.file("one.wav", 2.0), station.file("two.wav", 1.0)];
    let ids = station.player.enqueue(tracks, None).unwrap();
    station.play(0.5);
    assert_eq!(
        station.player.snapshot().cued.map(|entry| entry.entry_id),
        Some(ids[1])
    );

    // A track inserted first takes the place of the cued one.
    let urgent = station.file("urgent.wav", 1.0);
    station.player.enqueue(vec![urgent], Some(0)).unwrap();
    assert!(matches!(
        station.player.remove(ids[0]),
        Err(PlayerError::OnAir(_))
    ));
    station.play(3.0);
    let order: Vec<String> = started(&station.events())
        .into_iter()
        .map(|(_, path)| path)
        .collect();
    assert_eq!(order, ["one.wav", "urgent.wav", "two.wav"]);
}

#[test]
fn unplayable_files_are_skipped() {
    let mut station = Station::new();
    std::fs::write(station.library.path().join("broken.mp3"), b"not audio").unwrap();
    let broken = TrackRequest {
        path: "broken.mp3".to_owned(),
        ..station.file("unused.wav", 0.1)
    };
    let tracks = vec![
        station.file("one.wav", 1.0),
        broken,
        station.file("two.wav", 1.0),
    ];
    station.player.enqueue(tracks, None).unwrap();
    station.play(2.5);
    let order: Vec<String> = started(&station.events())
        .into_iter()
        .map(|(_, path)| path)
        .collect();
    assert_eq!(order, ["one.wav", "two.wav"]);
}

#[test]
fn paths_cannot_leave_the_library() {
    let root = Path::new("/music");
    assert_eq!(
        resolve_library_path(root, "Jazz/So What.flac"),
        Some(PathBuf::from("/music/Jazz/So What.flac"))
    );
    for path in [
        "../etc/passwd",
        "/etc/passwd",
        "a//b",
        "a/./b",
        "",
        "a\\..\\b",
    ] {
        assert_eq!(resolve_library_path(root, path), None, "{path}");
    }
    let mut station = Station::new();
    let mut track = station.file("one.wav", 0.1);
    track.path = "../one.wav".to_owned();
    assert!(matches!(
        station.player.enqueue(vec![track], None),
        Err(PlayerError::InvalidPath(_))
    ));
}

#[test]
fn fx_plays_over_the_music() {
    let mut station = Station::new();
    let music = station.file("music.wav", 2.0);
    let jingle = station.file("jingle.wav", 0.5);
    station.player.enqueue(vec![music], None).unwrap();
    station.play(0.3);
    station.player.play_fx(jingle).unwrap();
    station.play(1.0);
    let events = station.events();
    assert!(started(&events).contains(&(Deck::Fx, "jingle.wav".to_owned())));
    // The FX player does not change what is on air.
    assert_eq!(
        station.now_playing.borrow().title.as_deref(),
        Some("music.wav")
    );
}
