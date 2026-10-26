// SPDX-License-Identifier: AGPL-3.0-or-later

//! Acceptance (M11, external FFmpeg encoder): a real `ffmpeg` encodes the
//! master bus; a binary lacking the codec is reported clearly; a child that
//! is killed is started again. Skipped where `ffmpeg` is not installed.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::PathBuf;
use std::time::{Duration, Instant};

use heron_core::{EncoderKind, EncoderProfile};
use heron_outputs::encoder::{EncodeError, Encoder, INPUT_RATE};
use heron_outputs::external::ExternalEncoder;
use serde_json::json;

fn ffmpeg() -> Option<PathBuf> {
    let found = std::process::Command::new("ffmpeg")
        .arg("-version")
        .output()
        .is_ok_and(|output| output.status.success());
    if !found {
        eprintln!("ffmpeg not installed, skipping");
    }
    found.then(|| PathBuf::from("ffmpeg"))
}

fn profile(codec: &str) -> EncoderProfile {
    EncoderProfile {
        id: 1,
        name: "AAC".to_owned(),
        encoder: EncoderKind::ExternalFfmpeg,
        bitrate_kbps: Some(96),
        sample_rate: 44_100,
        channels: 2,
        options: json!({ "codec": codec }),
    }
}

/// 10 ms of a 1 kHz tone, interleaved stereo at 48 kHz.
fn block(n: &mut usize) -> Vec<f32> {
    (0..INPUT_RATE as usize / 100)
        .flat_map(|_| {
            let t = *n as f32 / INPUT_RATE as f32;
            *n += 1;
            let value = 0.5 * (std::f32::consts::TAU * 1_000.0 * t).sin();
            [value, value]
        })
        .collect()
}

/// Feeds `seconds` of audio, a little faster than real time.
fn feed(encoder: &mut ExternalEncoder, seconds: usize, out: &mut Vec<u8>) -> Vec<String> {
    let mut errors = Vec::new();
    let mut n = 0;
    for _ in 0..seconds * 100 {
        match encoder.encode(&block(&mut n), out) {
            Ok(()) | Err(EncodeError::Waiting) => {}
            Err(error) => errors.push(error.to_string()),
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    errors
}

/// ADTS frames in `stream`, checking each header's sync word and length.
fn adts_frames(stream: &[u8]) -> usize {
    let mut frames = 0;
    let mut at = 0;
    while at + 7 <= stream.len() {
        assert_eq!(stream[at], 0xFF, "sync word at byte {at}");
        assert_eq!(stream[at + 1] & 0xF0, 0xF0, "sync word at byte {at}");
        let length = (usize::from(stream[at + 3] & 0x03) << 11)
            | (usize::from(stream[at + 4]) << 3)
            | (usize::from(stream[at + 5]) >> 5);
        assert!(length >= 7, "frame length {length}");
        at += length;
        frames += 1;
    }
    frames
}

#[test]
fn encodes_aac_with_ffmpeg() {
    let Some(program) = ffmpeg() else {
        return;
    };
    let mut encoder = ExternalEncoder::with_program(&profile("aac"), program).unwrap();
    assert_eq!(encoder.info().content_type, "audio/aac");
    let mut aac = Vec::new();
    let errors = feed(&mut encoder, 3, &mut aac);
    assert!(errors.is_empty(), "{errors:?}");
    encoder.flush(&mut aac).unwrap();
    assert_eq!(encoder.pid(), None, "the child is gone after a flush");
    // 3 s at 44.1 kHz: 129 frames of 1,024 samples, give or take the
    // encoder's delay; 96 kbps is about 36 kB.
    let frames = adts_frames(&aac);
    assert!((120..=135).contains(&frames), "{frames} frames");
    assert!(
        (25_000..=50_000).contains(&aac.len()),
        "{} bytes",
        aac.len()
    );
}

#[test]
fn reports_a_missing_codec_and_waits_before_trying_again() {
    let Some(program) = ffmpeg() else {
        return;
    };
    let mut encoder = ExternalEncoder::with_program(&profile("no_such_codec"), program).unwrap();
    let mut out = Vec::new();
    let error = encoder.encode(&block(&mut 0), &mut out).unwrap_err();
    assert!(
        error.to_string().contains("no no_such_codec encoder"),
        "{error}"
    );
    assert!(
        matches!(
            encoder.encode(&block(&mut 0), &mut out),
            Err(EncodeError::Waiting)
        ),
        "no second attempt right away"
    );
    assert_eq!(encoder.pid(), None);
}

#[test]
fn reports_a_missing_binary() {
    let mut encoder =
        ExternalEncoder::with_program(&profile("aac"), PathBuf::from("/nonexistent/ffmpeg"))
            .unwrap();
    let error = encoder.encode(&block(&mut 0), &mut Vec::new()).unwrap_err();
    assert!(error.to_string().contains("HERON_FFMPEG"), "{error}");
}

#[test]
fn a_killed_ffmpeg_is_started_again() {
    let Some(program) = ffmpeg() else {
        return;
    };
    let mut encoder = ExternalEncoder::with_program(&profile("aac"), program).unwrap();
    let mut out = Vec::new();
    assert!(feed(&mut encoder, 1, &mut out).is_empty());
    let first = encoder.pid().unwrap();
    std::process::Command::new("kill")
        .args(["-9", &first.to_string()])
        .status()
        .unwrap();

    // The failure is reported, then a new child encodes after the delay.
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut errors = Vec::new();
    let mut n = 0;
    let mut restarted = Vec::new();
    while Instant::now() < deadline {
        let mut chunk = Vec::new();
        match encoder.encode(&block(&mut n), &mut chunk) {
            Ok(()) | Err(EncodeError::Waiting) => {}
            Err(error) => errors.push(error.to_string()),
        }
        if encoder.pid().is_some_and(|pid| pid != first) {
            restarted.extend_from_slice(&chunk);
            if restarted.len() > 4_000 {
                break;
            }
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(
        errors.iter().any(|error| error.contains("ffmpeg exited")),
        "{errors:?}"
    );
    assert!(
        restarted.len() > 4_000,
        "{} bytes after the restart",
        restarted.len()
    );
    assert_eq!(
        (restarted[0], restarted[1] & 0xF0),
        (0xFF, 0xF0),
        "a fresh ADTS stream"
    );
}
