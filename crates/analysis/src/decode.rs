// SPDX-License-Identifier: AGPL-3.0-or-later

//! Decoding files with Symphonia and streaming them into the [`Analyzer`].

use std::fs::File;
use std::io::ErrorKind;
use std::path::Path;

use heron_core::AnalysisSettings;
use symphonia::core::codecs::audio::AudioDecoderOptions;
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, TrackType};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;

use crate::{Analysis, Analyzer, Error};

/// Decodes `path` and analyzes it. CPU-bound: run it on a blocking thread.
pub fn analyze_file(path: &Path, settings: &AnalysisSettings) -> Result<Analysis, Error> {
    match decode_and_analyze(path, settings, true) {
        // Some MP3s carry a broken Xing/LAME header whose frame count makes
        // gapless trimming drop every sample. Decode them as they are.
        Err(Error::Empty) => {
            tracing::warn!(path = %path.display(), "no audio with gapless decoding, retrying without");
            decode_and_analyze(path, settings, false)
        }
        result => result,
    }
}

fn decode_and_analyze(
    path: &Path,
    settings: &AnalysisSettings,
    gapless: bool,
) -> Result<Analysis, Error> {
    let file = File::open(path)?;
    let stream = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(extension) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(extension);
    }
    let mut format = symphonia::default::get_probe().probe(
        &hint,
        stream,
        FormatOptions::default(),
        MetadataOptions::default(),
    )?;

    let track = format
        .default_track(TrackType::Audio)
        .ok_or(Error::NoAudioTrack)?;
    let track_id = track.id;
    let codec_params = track
        .codec_params
        .as_ref()
        .and_then(|params| params.audio())
        .ok_or(Error::NoAudioTrack)?;
    // Gapless: drop encoder delay and padding (LAME headers, AAC edit lists),
    // so cue points line up with what the engine plays.
    let decoder_options = AudioDecoderOptions::default().gapless(gapless);
    let mut decoder =
        symphonia::default::get_codecs().make_audio_decoder(codec_params, &decoder_options)?;

    let mut analyzer: Option<Analyzer> = None;
    let mut samples: Vec<f32> = Vec::new();
    let mut skipped_packets = 0_u64;
    loop {
        let packet = match format.next_packet() {
            Ok(Some(packet)) => packet,
            Ok(None) => break,
            // Truncated files: analyze what could be read.
            Err(SymphoniaError::IoError(error)) if error.kind() == ErrorKind::UnexpectedEof => {
                break;
            }
            Err(error) => return Err(error.into()),
        };
        if packet.track_id != track_id {
            continue;
        }
        let buffer = match decoder.decode(&packet) {
            Ok(buffer) => buffer,
            Err(SymphoniaError::DecodeError(reason)) => {
                if skipped_packets == 0 {
                    tracing::debug!(path = %path.display(), reason, "undecodable packet");
                }
                skipped_packets += 1;
                continue;
            }
            Err(error) => return Err(error.into()),
        };

        let rate = buffer.spec().rate();
        let channels = buffer.spec().channels().count();
        if analyzer.is_none() {
            analyzer = Some(Analyzer::new(rate, channels)?);
        }
        let Some(analyzer) = analyzer.as_mut() else {
            continue;
        };
        if analyzer.rate() != rate || analyzer.channels() != channels {
            return Err(Error::FormatChanged);
        }
        samples.resize(buffer.samples_interleaved(), 0.0);
        buffer.copy_to_slice_interleaved(&mut samples);
        analyzer.push(&samples)?;
    }

    if skipped_packets > 0 {
        tracing::warn!(path = %path.display(), skipped_packets, "skipped undecodable packets");
    }
    analyzer.ok_or(Error::Empty)?.finish(settings)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_wav(path: &Path, rate: u32, segments: &[(f64, f32)]) {
        let spec = hound::WavSpec {
            channels: 2,
            sample_rate: rate,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = hound::WavWriter::create(path, spec).unwrap();
        let mut n = 0_u64;
        for &(seconds, amplitude) in segments {
            for _ in 0..(seconds * f64::from(rate)) as u64 {
                let t = n as f64 / f64::from(rate);
                let value = amplitude * (std::f64::consts::TAU * 440.0 * t).sin() as f32;
                let sample = (value * f32::from(i16::MAX)) as i16;
                writer.write_sample(sample).unwrap();
                writer.write_sample(sample).unwrap();
                n += 1;
            }
        }
        writer.finalize().unwrap();
    }

    #[test]
    fn analyzes_a_wav_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("track.wav");
        write_wav(&path, 44_100, &[(1.5, 0.0), (6.0, 0.4), (2.0, 0.0)]);

        let analysis = analyze_file(&path, &AnalysisSettings::default()).unwrap();
        assert_eq!(analysis.duration_ms, 9_500);
        assert!(analysis.cue_in_ms.abs_diff(1_500) <= 6, "{analysis:?}");
        assert!(analysis.cue_out_ms.abs_diff(7_500) <= 6, "{analysis:?}");
        assert!(analysis.integrated_lufs.is_some());
    }

    const MP3_FIXTURE: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/silence-1s-sine-3s.mp3"
    );

    /// An ID3v2.3 tag holding `art_bytes` of cover art (APIC frame).
    fn id3v2_tag_with_cover(art_bytes: usize) -> Vec<u8> {
        let mut frame_body = b"\0image/png\0\x03\0".to_vec();
        frame_body.resize(frame_body.len() + art_bytes, 0);
        let mut frame = b"APIC".to_vec();
        frame.extend((frame_body.len() as u32).to_be_bytes());
        frame.extend([0, 0]);
        frame.extend(frame_body);

        let size = frame.len() as u32;
        let syncsafe = [
            ((size >> 21) & 0x7f) as u8,
            ((size >> 14) & 0x7f) as u8,
            ((size >> 7) & 0x7f) as u8,
            (size & 0x7f) as u8,
        ];
        let mut tag = b"ID3\x03\0\0".to_vec();
        tag.extend(syncsafe);
        tag.extend(frame);
        tag
    }

    #[test]
    fn analyzes_an_mp3_file() {
        let analysis = analyze_file(Path::new(MP3_FIXTURE), &AnalysisSettings::default()).unwrap();
        assert!(analysis.duration_ms.abs_diff(4_000) <= 30, "{analysis:?}");
        assert!(analysis.cue_in_ms.abs_diff(1_000) <= 30, "{analysis:?}");
    }

    #[test]
    fn skips_large_id3v2_tags() {
        // Real files often start with megabytes of embedded cover art.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("with-cover.mp3");
        let mut bytes = id3v2_tag_with_cover(3 * 1024 * 1024);
        bytes.extend(std::fs::read(MP3_FIXTURE).unwrap());
        std::fs::write(&path, bytes).unwrap();

        let analysis = analyze_file(&path, &AnalysisSettings::default()).unwrap();
        assert!(analysis.duration_ms.abs_diff(4_000) <= 30, "{analysis:?}");
        assert!(analysis.cue_in_ms.abs_diff(1_000) <= 30, "{analysis:?}");
    }

    #[test]
    fn rejects_files_that_are_not_audio() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("notes.mp3");
        std::fs::write(&path, b"definitely not an mp3 file").unwrap();
        let error = analyze_file(&path, &AnalysisSettings::default()).unwrap_err();
        assert!(!error.is_transient(), "{error}");
    }

    #[test]
    fn missing_files_are_transient_errors() {
        let error = analyze_file(
            Path::new("/nonexistent/track.flac"),
            &AnalysisSettings::default(),
        )
        .unwrap_err();
        assert!(error.is_transient());
    }
}
