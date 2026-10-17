// SPDX-License-Identifier: AGPL-3.0-or-later

//! Decode threads: one per loaded track, decoding ahead into a ring buffer
//! that the deck reads on the audio thread.
//!
//! Decoding goes through `heron_analysis::Decoder`, the code the analysis
//! used, so cue points line up with the audio.

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
    /// Starts decoding `path` from `cue_in_ms` to `cue_out_ms` on a new thread.
    pub fn spawn(path: PathBuf, cue_in_ms: u32, cue_out_ms: u32) -> std::io::Result<Self> {
        let (producer, consumer) =
            rtrb::RingBuffer::new(BUFFER_SECONDS * SAMPLE_RATE as usize * CHANNELS);
        let state = Arc::new(DecodeState::default());
        let thread_state = Arc::clone(&state);
        let name = path
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
                let result = decode(&path, range, &mut writer);
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
    if let Err(error) = decode(path, range, &mut audio) {
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
    path: &Path,
    range: std::ops::Range<u64>,
    writer: &mut impl Output,
) -> Result<(), DecodeError> {
    let mut decoder = Decoder::open(path)?;
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

    while let Some(block) = decoder.next_block()? {
        to_stereo(block.samples, block.channels, &mut stereo);
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

        let source = DecodedSource::spawn(path, 500, 2_250).unwrap();
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
        let audio = read_all(DecodedSource::spawn(path, 0, 60_000).unwrap());
        assert_eq!(audio.len(), 48_000 * CHANNELS);
    }

    #[test]
    fn missing_files_end_at_once_and_report_failure() {
        let source = DecodedSource::spawn(PathBuf::from("/nonexistent.mp3"), 0, 1_000).unwrap();
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
