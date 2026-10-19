// SPDX-License-Identifier: AGPL-3.0-or-later

//! Decode threads: one per loaded track, decoding ahead into a ring buffer
//! that the deck reads on the audio thread.
//!
//! Decoding goes through `heron_analysis::Decoder`, the code the analysis
//! used, so cue points line up with the audio. A file read into memory
//! beforehand ([`crate::preload`]) is decoded from there, so the track plays
//! to the end even if the library becomes unreachable.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use heron_analysis::Decoder;
use heron_dsp::StreamResampler;

use crate::mixer::{AudioSource, CHANNELS, SAMPLE_RATE, ms_to_frames};

/// Audio decoded ahead of the deck.
const BUFFER_SECONDS: usize = 10;
/// Samples further than this from zero are corrupt data, not music. Real
/// masters peak a few dB over full scale at most (the analysis rejects
/// files above +12 dBTP).
const MAX_SAMPLE: f32 = 4.0;
/// Audio decoded before a track may start: 100 ms.
const READY_SAMPLES: usize = SAMPLE_RATE as usize / 10 * CHANNELS;
/// How long the decoder sleeps when the buffer is full.
const FULL_WAIT: Duration = Duration::from_millis(5);
/// Starting further into the file than this (resuming after a restart),
/// the audio before the start is decoded but not resampled, to get there
/// quickly. Resampling starts [`SKIP_MARGIN`] before the start.
const SKIP_FROM: u64 = 30 * SAMPLE_RATE as u64;
const SKIP_MARGIN: u64 = SAMPLE_RATE as u64;

/// A file to play, with its content when it was read into memory beforehand.
#[derive(Clone, Debug, Default)]
pub struct SourceFile {
    pub path: PathBuf,
    pub contents: Option<Arc<[u8]>>,
}

impl SourceFile {
    pub fn on_disk(path: PathBuf) -> Self {
        Self {
            path,
            contents: None,
        }
    }
}

#[derive(Debug, Default)]
struct DecodeState {
    /// The decoder pushed everything it will ever push.
    done: AtomicBool,
    /// Decoding failed (the track ends where the error happened).
    failed: AtomicBool,
    /// The deck no longer wants the audio.
    cancelled: AtomicBool,
}

/// The deck's end of a decode thread.
pub struct DecodedSource {
    consumer: rtrb::Consumer<f32>,
    state: Arc<DecodeState>,
}

impl DecodedSource {
    /// Starts decoding `file` from `cue_in_ms` to `cue_out_ms` on a new thread.
    pub fn spawn(file: SourceFile, cue_in_ms: u32, cue_out_ms: u32) -> std::io::Result<Self> {
        let (producer, consumer) =
            rtrb::RingBuffer::new(BUFFER_SECONDS * SAMPLE_RATE as usize * CHANNELS);
        let state = Arc::new(DecodeState::default());
        let thread_state = Arc::clone(&state);
        let name = file
            .path
            .file_name()
            .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
        std::thread::Builder::new()
            .name("heron-decode".to_owned())
            .spawn(move || {
                let range = ms_to_frames(cue_in_ms)..ms_to_frames(cue_out_ms);
                let mut writer = Writer {
                    producer,
                    state: &thread_state,
                };
                let result = decode(&file, range, &mut writer);
                if let Err(error) = &result {
                    thread_state.failed.store(true, Ordering::Release);
                    tracing::warn!(file = %name, %error, "decoding failed");
                }
                thread_state.done.store(true, Ordering::Release);
            })?;
        Ok(Self { consumer, state })
    }

    /// Decoding failed. The track may still have played up to the error.
    pub fn has_failed(&self) -> bool {
        self.state.failed.load(Ordering::Acquire)
    }

    /// Frames decoded and waiting to be played.
    pub fn buffered_frames(&self) -> usize {
        self.consumer.slots() / CHANNELS
    }
}

impl AudioSource for DecodedSource {
    fn read(&mut self, out: &mut [f32]) -> usize {
        let available = self.consumer.slots().min(out.len());
        let available = available - available % CHANNELS;
        let Ok(chunk) = self.consumer.read_chunk(available) else {
            return 0;
        };
        let (first, second) = chunk.as_slices();
        out[..first.len()].copy_from_slice(first);
        out[first.len()..available].copy_from_slice(second);
        chunk.commit_all();
        available / CHANNELS
    }

    fn is_finished(&self) -> bool {
        // Read `done` first: the decoder sets it after its last push.
        self.state.done.load(Ordering::Acquire) && self.consumer.is_empty()
    }

    fn is_ready(&self) -> bool {
        self.state.done.load(Ordering::Acquire) || self.consumer.slots() >= READY_SAMPLES
    }
}

impl Drop for DecodedSource {
    fn drop(&mut self) {
        self.state.cancelled.store(true, Ordering::Release);
    }
}

#[derive(Debug, thiserror::Error)]
enum DecodeError {
    #[error(transparent)]
    Decode(#[from] heron_analysis::Error),
    #[error(transparent)]
    Resample(#[from] heron_dsp::resample::Error),
}

/// Where decoded audio goes.
trait Output {
    /// Takes `samples`; returns `false` when no more audio is wanted.
    fn write(&mut self, samples: &[f32]) -> bool;
}

impl Output for Vec<f32> {
    fn write(&mut self, samples: &[f32]) -> bool {
        self.extend_from_slice(samples);
        true
    }
}

/// Decodes the part of `path` between the cue points into memory, on the
/// calling thread. Same audio as [`DecodedSource`]; for offline rendering
/// and tests.
pub fn decode_to_memory(path: &Path, cue_in_ms: u32, cue_out_ms: u32) -> Vec<f32> {
    let mut audio = Vec::new();
    let range = ms_to_frames(cue_in_ms)..ms_to_frames(cue_out_ms);
    if let Err(error) = decode(&SourceFile::on_disk(path.to_path_buf()), range, &mut audio) {
        tracing::warn!(path = %path.display(), %error, "decoding failed");
    }
    audio
}

struct Writer<'a> {
    producer: rtrb::Producer<f32>,
    state: &'a DecodeState,
}

impl Output for Writer<'_> {
    /// Pushes `samples`, waiting for room. Returns `false` when the deck no
    /// longer wants the audio.
    fn write(&mut self, mut samples: &[f32]) -> bool {
        while !samples.is_empty() {
            if self.state.cancelled.load(Ordering::Acquire) || self.producer.is_abandoned() {
                return false;
            }
            let room = self.producer.slots().min(samples.len());
            let room = room - room % CHANNELS;
            if room == 0 {
                std::thread::sleep(FULL_WAIT);
                continue;
            }
            let Ok(mut chunk) = self.producer.write_chunk(room) else {
                continue;
            };
            let (first, second) = chunk.as_mut_slices();
            let split = first.len();
            first.copy_from_slice(&samples[..split]);
            second.copy_from_slice(&samples[split..room]);
            chunk.commit_all();
            samples = &samples[room..];
        }
        true
    }
}

/// Decodes the frames of `range` (at the internal rate) into `writer`.
fn decode(
    file: &SourceFile,
    range: std::ops::Range<u64>,
    writer: &mut impl Output,
) -> Result<(), DecodeError> {
    let mut decoder = match &file.contents {
        Some(contents) => Decoder::from_memory(&file.path, Arc::clone(contents))?,
        None => Decoder::open(&file.path)?,
    };
    let mut resampler: Option<StreamResampler> = None;
    let mut stereo = Vec::new();
    let mut resampled = Vec::new();
    // Frames at the internal rate produced so far, before cutting.
    let mut position = 0_u64;

    let mut emit = |audio: &[f32], position: &mut u64| -> bool {
        let frames = (audio.len() / CHANNELS) as u64;
        let start = range.start.saturating_sub(*position).min(frames);
        let end = range.end.saturating_sub(*position).min(frames);
        *position += frames;
        if start < end {
            let slice = &audio[start as usize * CHANNELS..end as usize * CHANNELS];
            if !writer.write(slice) {
                return false;
            }
        }
        *position < range.end
    };

    // Input frames skipped without resampling.
    let mut skipped = 0_u64;

    while let Some(block) = decoder.next_block()? {
        let mut samples = block.samples;
        if resampler.is_none() && range.start > SKIP_FROM {
            let rate = u64::from(block.rate);
            let channels = block.channels.max(1);
            let frames = (samples.len() / channels) as u64;
            let end = (skipped + frames) * u64::from(SAMPLE_RATE) / rate;
            if end + SKIP_MARGIN < range.start {
                skipped += frames;
                continue;
            }
            // Start on an input frame that falls exactly on an output frame
            // (every 147 frames at 44.1 kHz), so the audio lines up with
            // what decoding from the start gives.
            let period = rate / gcd(rate, u64::from(SAMPLE_RATE));
            let drop = ((period - skipped % period) % period).min(frames);
            skipped += drop;
            samples = &samples[drop as usize * channels..];
            position = skipped * u64::from(SAMPLE_RATE) / rate;
        }
        to_stereo(samples, block.channels, &mut stereo);
        let resampler = match &mut resampler {
            Some(resampler) => resampler,
            None => resampler.insert(StreamResampler::new(block.rate, SAMPLE_RATE, CHANNELS)?),
        };
        resampled.clear();
        resampler.process(&stereo, &mut resampled)?;
        if !emit(&resampled, &mut position) {
            return Ok(());
        }
    }
    if let Some(resampler) = &mut resampler {
        resampled.clear();
        resampler.finish(&mut resampled)?;
        emit(&resampled, &mut position);
    }
    Ok(())
}

fn gcd(a: u64, b: u64) -> u64 {
    if b == 0 { a.max(1) } else { gcd(b, a % b) }
}

/// Converts interleaved audio of any channel count to stereo, and clamps
/// corrupt samples.
fn to_stereo(samples: &[f32], channels: usize, out: &mut Vec<f32>) {
    out.clear();
    let clamp = |sample: f32| sample.clamp(-MAX_SAMPLE, MAX_SAMPLE);
    match channels {
        1 => out.extend(samples.iter().flat_map(|&s| [clamp(s), clamp(s)])),
        // Front left and right of surround layouts.
        _ => out.extend(
            samples
                .chunks_exact(channels.max(2))
                .flat_map(|frame| [clamp(frame[0]), clamp(frame[1])]),
        ),
    }
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use super::*;

    fn write_wav(path: &Path, rate: u32, channels: u16, seconds: f64) {
        let spec = hound::WavSpec {
            channels,
            sample_rate: rate,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = hound::WavWriter::create(path, spec).unwrap();
        for n in 0..(seconds * f64::from(rate)) as u64 {
            let t = n as f64 / f64::from(rate);
            let value = 0.5 * (std::f64::consts::TAU * 440.0 * t).sin();
            for _ in 0..channels {
                writer
                    .write_sample((value * f64::from(i16::MAX)) as i16)
                    .unwrap();
            }
        }
        writer.finalize().unwrap();
    }

    fn read_all(mut source: DecodedSource) -> Vec<f32> {
        let mut all = Vec::new();
        let mut block = vec![0.0; 4_800];
        let deadline = Instant::now() + Duration::from_secs(20);
        while !source.is_finished() {
            assert!(Instant::now() < deadline, "decoding never finished");
            let frames = source.read(&mut block);
            all.extend_from_slice(&block[..frames * CHANNELS]);
            if frames == 0 {
                std::thread::sleep(Duration::from_millis(1));
            }
        }
        all
    }

    #[test]
    fn decodes_between_the_cue_points_at_the_internal_rate() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("track.wav");
        write_wav(&path, 44_100, 1, 3.0);

        let source = DecodedSource::spawn(SourceFile::on_disk(path), 500, 2_250).unwrap();
        let audio = read_all(source);
        assert_eq!(audio.len(), 1_750 * 48 * CHANNELS);
        // Mono plays on both channels.
        assert!(audio.chunks(2).all(|frame| frame[0] == frame[1]));
    }

    #[test]
    fn cue_out_past_the_end_stops_at_the_end() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("track.wav");
        write_wav(&path, 48_000, 2, 1.0);
        let audio = read_all(DecodedSource::spawn(SourceFile::on_disk(path), 0, 60_000).unwrap());
        assert_eq!(audio.len(), 48_000 * CHANNELS);
    }

    #[test]
    fn decodes_preloaded_files_without_the_disk() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("track.wav");
        write_wav(&path, 48_000, 2, 1.0);
        let file = SourceFile {
            contents: Some(std::fs::read(&path).unwrap().into()),
            path: path.clone(),
        };
        std::fs::remove_file(&path).unwrap();
        let source = DecodedSource::spawn(file, 0, 60_000).unwrap();
        assert_eq!(read_all(source).len(), 48_000 * CHANNELS);
    }

    #[test]
    fn far_starts_skip_ahead_to_the_same_audio() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("track.wav");
        write_wav(&path, 44_100, 2, 40.0);
        let whole = decode_to_memory(&path, 0, 40_000);
        let tail =
            read_all(DecodedSource::spawn(SourceFile::on_disk(path), 35_000, 40_000).unwrap());
        // The file ends there: the fresh resampler may round its last frame away.
        assert!(tail.len().abs_diff(5_000 * 48 * CHANNELS) <= CHANNELS);
        let start = 35_000 * 48 * CHANNELS;
        // The resampler starts afresh, 1 s before: the same audio, give or
        // take the resampler's rounding.
        let error = whole[start..start + tail.len()]
            .iter()
            .zip(&tail)
            .map(|(a, b)| (a - b).abs())
            .fold(0.0_f32, f32::max);
        assert!(error < 1e-3, "max difference {error}");
    }

    #[test]
    fn missing_files_end_at_once_and_report_failure() {
        let source =
            DecodedSource::spawn(SourceFile::on_disk("/nonexistent.mp3".into()), 0, 1_000).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while !source.is_finished() {
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(source.has_failed());
    }

    #[test]
    fn corrupt_samples_are_clamped() {
        let mut out = Vec::new();
        to_stereo(&[1e9, -1e9, 0.5, 0.25], 2, &mut out);
        assert_eq!(out, [MAX_SAMPLE, -MAX_SAMPLE, 0.5, 0.25]);
        to_stereo(&[0.1, 0.2, 0.3, 0.4, 0.5, 0.6], 3, &mut out);
        assert_eq!(out, [0.1, 0.2, 0.4, 0.5]);
    }
}
