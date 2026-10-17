// SPDX-License-Identifier: AGPL-3.0-or-later

//! Streaming sample rate conversion of interleaved audio.
//!
//! Used outside the audio thread: by the decoders (file rate to the internal
//! 48 kHz) and the encoders (48 kHz to the profile's rate).

use audioadapter_buffers::direct::InterleavedSlice;
use rubato::{Fft, FixedSync, Indexing, Resampler};

/// Frames per resampler call.
const CHUNK_FRAMES: usize = 1024;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("cannot resample from {from} Hz to {to} Hz with {channels} channels: {reason}")]
    Unsupported {
        from: u32,
        to: u32,
        channels: usize,
        reason: String,
    },
    #[error("resampling failed: {0}")]
    Resample(#[from] rubato::ResampleError),
    #[error("resampler buffer: {0}")]
    Buffer(String),
}

/// Converts a stream of interleaved samples from one rate to another.
///
/// The output is aligned with the input (the resampler delay is removed) and,
/// once [`finish`](Self::finish) is called, holds exactly
/// `round(input_frames * to / from)` frames, so positions in milliseconds mean
/// the same before and after.
pub struct StreamResampler {
    channels: usize,
    from: u32,
    to: u32,
    resampler: Option<Fft<f32>>,
    pending: Vec<f32>,
    scratch: Vec<f32>,
    trim: Trim,
    input_frames: u64,
}

impl StreamResampler {
    pub fn new(from: u32, to: u32, channels: usize) -> Result<Self, Error> {
        let unsupported = |reason: String| Error::Unsupported {
            from,
            to,
            channels,
            reason,
        };
        if channels == 0 || from == 0 || to == 0 {
            return Err(unsupported("invalid format".to_owned()));
        }
        let resampler = if from == to {
            None
        } else {
            Some(
                Fft::<f32>::new(
                    from as usize,
                    to as usize,
                    CHUNK_FRAMES,
                    channels,
                    FixedSync::Input,
                )
                .map_err(|error| unsupported(error.to_string()))?,
            )
        };
        let (delay, scratch_frames) = resampler.as_ref().map_or((0, 0), |resampler| {
            (resampler.output_delay(), resampler.output_frames_max())
        });
        Ok(Self {
            channels,
            from,
            to,
            resampler,
            pending: Vec::new(),
            scratch: vec![0.0; scratch_frames * channels],
            trim: Trim {
                channels,
                delay,
                frames: 0,
            },
            input_frames: 0,
        })
    }

    pub fn channels(&self) -> usize {
        self.channels
    }

    /// Resamples `input` and appends the result to `output`. A trailing
    /// partial frame is ignored.
    pub fn process(&mut self, input: &[f32], output: &mut Vec<f32>) -> Result<(), Error> {
        let whole = input.len() - input.len() % self.channels;
        let input = &input[..whole];
        self.input_frames += (whole / self.channels) as u64;
        let Some(resampler) = self.resampler.as_mut() else {
            output.extend_from_slice(input);
            self.trim.frames += (whole / self.channels) as u64;
            return Ok(());
        };
        self.pending.extend_from_slice(input);
        let mut offset = 0;
        loop {
            let needed = resampler.input_frames_next();
            if self.pending.len() - offset < needed * self.channels {
                break;
            }
            let chunk = &self.pending[offset..offset + needed * self.channels];
            let produced = run(
                resampler,
                chunk,
                needed,
                None,
                &mut self.scratch,
                self.channels,
            )?;
            offset += needed * self.channels;
            emit(
                &mut self.trim,
                &self.scratch[..produced * self.channels],
                output,
                u64::MAX,
            );
        }
        self.pending.drain(..offset);
        Ok(())
    }

    /// Flushes the end of the stream into `output`.
    pub fn finish(&mut self, output: &mut Vec<f32>) -> Result<(), Error> {
        let expected =
            (self.input_frames as f64 * f64::from(self.to) / f64::from(self.from)).round() as u64;
        let Some(resampler) = self.resampler.as_mut() else {
            return Ok(());
        };
        let pending = std::mem::take(&mut self.pending);
        let mut offset = 0;
        while self.trim.frames < expected {
            let needed = resampler.input_frames_next();
            let len = ((pending.len() - offset) / self.channels).min(needed);
            let produced = run(
                resampler,
                &pending[offset..offset + len * self.channels],
                needed,
                Some(len),
                &mut self.scratch,
                self.channels,
            )?;
            offset += len * self.channels;
            if produced == 0 {
                break;
            }
            emit(
                &mut self.trim,
                &self.scratch[..produced * self.channels],
                output,
                expected,
            );
        }
        Ok(())
    }
}

/// Output bookkeeping: the resampler delay still to drop and the frames written.
struct Trim {
    channels: usize,
    delay: usize,
    frames: u64,
}

/// Appends `produced` (interleaved), minus the delay still to drop, up to
/// `limit` output frames in total.
fn emit(trim: &mut Trim, produced: &[f32], output: &mut Vec<f32>, limit: u64) {
    let produced_frames = produced.len() / trim.channels;
    let skip = trim.delay.min(produced_frames);
    trim.delay -= skip;
    let available = (produced_frames - skip) as u64;
    let frames = available.min(limit.saturating_sub(trim.frames)) as usize;
    let start = skip * trim.channels;
    output.extend_from_slice(&produced[start..start + frames * trim.channels]);
    trim.frames += frames as u64;
}

/// One resampler call on `frames` frames of `input` (fewer when `partial` is set).
fn run(
    resampler: &mut Fft<f32>,
    input: &[f32],
    frames: usize,
    partial: Option<usize>,
    scratch: &mut [f32],
    channels: usize,
) -> Result<usize, Error> {
    let valid = partial.unwrap_or(frames);
    let input = InterleavedSlice::new(input, channels, valid)
        .map_err(|error| Error::Buffer(error.to_string()))?;
    let out_frames = scratch.len() / channels;
    let mut output = InterleavedSlice::new_mut(scratch, channels, out_frames)
        .map_err(|error| Error::Buffer(error.to_string()))?;
    let indexing = Indexing {
        partial_len: partial,
        ..Indexing::default()
    };
    let (_, produced) = resampler.process_into_buffer(&input, &mut output, Some(&indexing))?;
    Ok(produced)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(rate: u32, frames: usize, freq: f64) -> Vec<f32> {
        (0..frames)
            .flat_map(|n| {
                let value =
                    (std::f64::consts::TAU * freq * n as f64 / f64::from(rate)).sin() as f32;
                [value * 0.5, value * 0.5]
            })
            .collect()
    }

    fn resample(from: u32, to: u32, input: &[f32], block: usize) -> Vec<f32> {
        let mut resampler = StreamResampler::new(from, to, 2).unwrap();
        let mut output = Vec::new();
        for chunk in input.chunks(block * 2) {
            resampler.process(chunk, &mut output).unwrap();
        }
        resampler.finish(&mut output).unwrap();
        output
    }

    #[test]
    fn output_length_matches_the_ratio() {
        for (frames, block) in [(44_100, 1_152), (10_000, 333), (100, 4_096), (1, 1)] {
            let input = sine(44_100, frames, 440.0);
            let output = resample(44_100, 48_000, &input, block);
            let expected = (frames as f64 * 48_000.0 / 44_100.0).round() as usize;
            assert_eq!(output.len(), expected * 2, "{frames} frames");
        }
    }

    #[test]
    fn output_is_aligned_with_the_input() {
        // Silence, then a tone starting at exactly 0.5 s.
        let mut input = vec![0.0; 22_050 * 2];
        input.extend(sine(44_100, 22_050, 1_000.0));
        let output = resample(44_100, 48_000, &input, 1_000);
        let onset = output
            .chunks(2)
            .position(|frame| frame[0].abs() > 0.05)
            .unwrap();
        assert!(onset.abs_diff(24_000) <= 5, "tone starts at frame {onset}");
    }

    #[test]
    fn same_rate_passes_through() {
        let input = sine(48_000, 1_000, 440.0);
        assert_eq!(resample(48_000, 48_000, &input, 100), input);
    }

    #[test]
    fn keeps_the_signal() {
        let input = sine(48_000, 48_000, 1_000.0);
        let output = resample(48_000, 44_100, &input, 480);
        // A 1 kHz tone at 0.5 keeps its RMS (0.354) away from the edges.
        let middle = &output[8_000..80_000];
        let rms = (middle.iter().map(|s| s * s).sum::<f32>() / middle.len() as f32).sqrt();
        assert!((rms - 0.3536).abs() < 0.005, "rms {rms}");
    }
}
