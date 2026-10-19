// SPDX-License-Identifier: AGPL-3.0-or-later

//! Tempo (BPM) estimation, streamed with the rest of the analysis.
//!
//! 1. Onset strength every 10 ms: the spectral flux, how much louder each
//!    frequency got since the previous frame (on a log scale, against its
//!    loudest neighbour there, so steady tones count for nothing), summed over
//!    the whole spectrum and over the bass alone. Hi-hats dominate the
//!    whole spectrum; the bass (kick drums, bass lines) marks the beat.
//! 2. The autocorrelation of both, each relative to its mean, over the
//!    whole track: a steady beat repeats every beat period.
//! 3. The tempo between 60 and 200 BPM whose period and its multiples (one
//!    to four beats) repeat best, weighted towards 120 BPM, where most
//!    music lies. The weight settles octave doubts (half or double tempo),
//!    not always right: the tempo can be corrected by hand.

use std::sync::Arc;

use realfft::{RealFftPlanner, RealToComplex};

/// Time between two onset strength values.
const HOP_S: f64 = 0.01;
/// Tempos considered.
const MIN_BPM: f64 = 60.0;
const MAX_BPM: f64 = 200.0;
/// Grid the tempo is searched on.
const BPM_STEP: f64 = 0.05;
/// Beat multiples whose repetition counts.
const MULTIPLES: usize = 4;
/// Tempo most music lies around, and how far (in octaves) it spreads.
const PRIOR_BPM: f64 = 120.0;
const PRIOR_OCTAVES: f64 = 1.5;
/// The bass, where the beat is clearest.
const BASS_FREQUENCY: f64 = 200.0;
/// Below this mean onset strength, the audio has no onsets to speak of
/// (a steady tone, silence).
const MIN_ONSET_STRENGTH: f64 = 1.0;
/// How much the onsets must repeat at the beat period, relative to their
/// energy (1: perfectly; drum patterns 0.75 and more, tones between
/// silences 0.12): below, no tempo rather than a wrong one.
const MIN_PERIODICITY: f64 = 0.2;
/// Shortest audio a tempo is measured on.
const MIN_SECONDS: f64 = 10.0;
/// Onset strengths are smoothed over this long.
const SMOOTHING_S: f64 = 0.05;
/// Onset strengths are taken relative to their mean over this long.
const LOCAL_MEAN_S: f64 = 0.5;
/// Frequencies above this carry little rhythm.
const MAX_FREQUENCY: f64 = 11_000.0;

pub struct TempoTracker {
    rate: u32,
    hop: usize,
    window: Vec<f32>,
    pending: Vec<f32>,
    fft: Arc<dyn RealToComplex<f32>>,
    input: Vec<f32>,
    spectrum: Vec<realfft::num_complex::Complex<f32>>,
    scratch: Vec<realfft::num_complex::Complex<f32>>,
    bins: usize,
    bass_bins: usize,
    levels: Vec<f32>,
    previous: Vec<f32>,
    has_previous: bool,
    flux: Vec<f32>,
    bass_flux: Vec<f32>,
}

impl TempoTracker {
    pub fn new(rate: u32) -> Self {
        let hop = ((f64::from(rate) * HOP_S).round() as usize).max(1);
        let size = (hop * 4).next_power_of_two();
        let fft = RealFftPlanner::<f32>::new().plan_fft_forward(size);
        let window = (0..size)
            .map(|n| {
                let phase = std::f32::consts::TAU * n as f32 / size as f32;
                0.5 - 0.5 * phase.cos()
            })
            .collect();
        let spectrum = fft.make_output_vec();
        let max_bin = (MAX_FREQUENCY * size as f64 / f64::from(rate)).floor() as usize;
        let bins = spectrum.len().min(max_bin.max(1));
        let bass_bins =
            ((BASS_FREQUENCY * size as f64 / f64::from(rate)).ceil() as usize).clamp(1, bins);
        Self {
            rate,
            hop,
            window,
            pending: Vec::with_capacity(size + hop),
            input: fft.make_input_vec(),
            scratch: fft.make_scratch_vec(),
            fft,
            spectrum,
            bins,
            bass_bins,
            levels: vec![0.0; bins],
            previous: vec![0.0; bins],
            has_previous: false,
            flux: Vec::new(),
            bass_flux: Vec::new(),
        }
    }

    /// Adds one mono sample.
    pub fn push(&mut self, sample: f32) {
        self.pending.push(sample);
        if self.pending.len() == self.window.len() {
            self.frame();
            self.pending.drain(..self.hop);
        }
    }

    fn frame(&mut self) {
        for ((input, &sample), &weight) in
            self.input.iter_mut().zip(&self.pending).zip(&self.window)
        {
            *input = sample * weight;
        }
        if self
            .fft
            .process_with_scratch(&mut self.input, &mut self.spectrum, &mut self.scratch)
            .is_err()
        {
            return;
        }
        for (level, bin) in self.levels.iter_mut().zip(&self.spectrum[..self.bins]) {
            *level = (1.0 + 100.0 * bin.norm()).ln();
        }
        let (mut flux, mut bass) = (0.0, 0.0);
        if self.has_previous {
            for (index, &level) in self.levels.iter().enumerate() {
                // Against the loudest neighbour of the previous frame: a
                // steady partial whose energy moves between neighbouring
                // bins (leakage, vibrato) is no onset (SuperFlux).
                let from = index.saturating_sub(1);
                let to = (index + 2).min(self.bins);
                let before = self.previous[from..to]
                    .iter()
                    .copied()
                    .fold(f32::MIN, f32::max);
                let rise = (level - before).max(0.0);
                flux += rise;
                if index < self.bass_bins {
                    bass += rise;
                }
            }
        }
        std::mem::swap(&mut self.previous, &mut self.levels);
        self.has_previous = true;
        self.flux.push(flux);
        self.bass_flux.push(bass);
    }

    /// The tempo, rounded to 0.1 BPM. `None` for audio too short, or
    /// without any repeating onsets.
    pub fn finish(self) -> Option<f64> {
        let hops_per_minute = 60.0 * f64::from(self.rate) / self.hop as f64;
        estimate(&self.flux, &self.bass_flux, hops_per_minute)
    }
}

/// Onset strengths above their local mean (so loud passages do not
/// dominate), and their mean.
fn onsets(flux: &[f32], hop_s: f64) -> (Vec<f64>, f64) {
    let half = ((LOCAL_MEAN_S / hop_s / 2.0).round() as usize).max(1);
    let mut prefix = Vec::with_capacity(flux.len() + 1);
    prefix.push(0.0_f64);
    for &value in flux {
        prefix.push(prefix[prefix.len() - 1] + f64::from(value));
    }
    let onsets: Vec<f64> = (0..flux.len())
        .map(|index| {
            let (from, to) = (
                index.saturating_sub(half),
                (index + half + 1).min(flux.len()),
            );
            let mean = (prefix[to] - prefix[from]) / (to - from) as f64;
            (f64::from(flux[index]) - mean).max(0.0)
        })
        .collect();
    let mean = onsets.iter().sum::<f64>() / onsets.len().max(1) as f64;
    (onsets, mean)
}

/// The tempo of onset strength curves (whole spectrum and bass) with
/// `hops_per_minute` values a minute.
fn estimate(flux: &[f32], bass_flux: &[f32], hops_per_minute: f64) -> Option<f64> {
    let hop_s = 60.0 / hops_per_minute;
    if (flux.len() as f64) * hop_s < MIN_SECONDS {
        return None;
    }
    let (all, all_mean) = onsets(flux, hop_s);
    if all_mean < MIN_ONSET_STRENGTH {
        return None;
    }
    let (bass, bass_mean) = onsets(bass_flux, hop_s);
    // Both count the same, whatever their scale.
    let combined: Vec<f64> = all
        .iter()
        .zip(&bass)
        .map(|(all, bass)| {
            all / all_mean
                + if bass_mean > 0.0 {
                    bass / bass_mean
                } else {
                    0.0
                }
        })
        .collect();
    // Around their mean: steady noise (a lossy codec on a tone) repeats at
    // every lag as much as at the beat, and must score nothing.
    // Smoothed over 50 ms: lossy codecs leave a ripple every frame (23 to
    // 26 ms) that repeats near every beat period too.
    let reach = ((SMOOTHING_S / hop_s / 2.0).round() as usize).max(1);
    let smoothed: Vec<f64> = (0..combined.len())
        .map(|index| {
            let (from, to) = (
                index.saturating_sub(reach),
                (index + reach + 1).min(combined.len()),
            );
            combined[from..to].iter().sum::<f64>() / (to - from) as f64
        })
        .collect();
    let mean = smoothed.iter().sum::<f64>() / smoothed.len() as f64;
    let onsets: Vec<f64> = smoothed.iter().map(|value| value - mean).collect();

    let longest_period = hops_per_minute / MIN_BPM;
    let max_lag = ((longest_period * MULTIPLES as f64).ceil() as usize + 2).min(onsets.len() / 2);
    let mut acf = vec![0.0_f64; max_lag + 1];
    for (lag, value) in acf.iter_mut().enumerate().skip(1) {
        let sum: f64 = onsets[lag..].iter().zip(&onsets).map(|(a, b)| a * b).sum();
        *value = sum / (onsets.len() - lag) as f64;
    }
    let energy = onsets.iter().map(|value| value * value).sum::<f64>() / onsets.len() as f64;
    if energy <= f64::EPSILON {
        return None;
    }
    let at = |lag: f64| -> f64 {
        let low = lag.floor() as usize;
        if low + 1 > max_lag {
            return 0.0;
        }
        let fraction = lag - low as f64;
        acf[low] * (1.0 - fraction) + acf[low + 1] * fraction
    };

    let steps = ((MAX_BPM - MIN_BPM) / BPM_STEP).round() as usize;
    let (best_bpm, _) = (0..=steps)
        .map(|step| {
            let bpm = MIN_BPM + step as f64 * BPM_STEP;
            let period = hops_per_minute / bpm;
            // The beat itself counts most: a pattern repeating every two
            // beats (snare on 2 and 4) must not halve the tempo.
            let repetition: f64 = (1..=MULTIPLES)
                .map(|k| at(period * k as f64) / k as f64)
                .sum();
            let octaves = (bpm / PRIOR_BPM).log2() / PRIOR_OCTAVES;
            let prior = (-0.5 * octaves * octaves).exp();
            (bpm, repetition * prior)
        })
        .fold((0.0, f64::MIN), |best, candidate| {
            if candidate.1 > best.1 {
                candidate
            } else {
                best
            }
        });
    // The beat must repeat: onsets that do not (a sound starting once,
    // codec noise on a steady tone) score near 0, drums near 1.
    let period = hops_per_minute / best_bpm;
    if at(period) < MIN_PERIODICITY * energy {
        return None;
    }
    Some((best_bpm * 10.0).round() / 10.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 44_100;

    /// A drum pattern at `bpm`: kick on every beat, snare on 2 and 4,
    /// closed hi-hat on eighth notes, a bass note, and some noise.
    fn drums(bpm: f64, seconds: f64) -> Vec<f32> {
        let len = (seconds * f64::from(RATE)) as usize;
        let beat = 60.0 / bpm;
        let mut out = vec![0.0_f32; len];
        let mut noise_state = 0x1234_5678_u32;
        let mut noise = move || {
            noise_state ^= noise_state << 13;
            noise_state ^= noise_state >> 17;
            noise_state ^= noise_state << 5;
            (noise_state as f32 / u32::MAX as f32) * 2.0 - 1.0
        };
        let mut add = |start: f64, length: f64, sound: &mut dyn FnMut(f64) -> f32| {
            let first = (start * f64::from(RATE)) as usize;
            let count = (length * f64::from(RATE)) as usize;
            for n in 0..count {
                if let Some(sample) = out.get_mut(first + n) {
                    *sample += sound(n as f64 / f64::from(RATE));
                }
            }
        };
        let beats = (seconds / beat) as usize;
        for index in 0..beats {
            let start = index as f64 * beat;
            add(start, 0.15, &mut |t| {
                let freq = 50.0 + 100.0 * (-t * 30.0).exp();
                (0.8 * (std::f64::consts::TAU * freq * t).sin() * (-t * 12.0).exp()) as f32
            });
            if index % 2 == 1 {
                add(start, 0.12, &mut |t| {
                    (0.4 * (-t * 25.0).exp()) as f32 * noise()
                });
            }
            for half in [0.0, 0.5] {
                add(start + half * beat, 0.04, &mut |t| {
                    (0.15 * (-t * 80.0).exp()) as f32 * noise()
                });
            }
        }
        for (n, sample) in out.iter_mut().enumerate() {
            let t = n as f64 / f64::from(RATE);
            *sample += (0.1 * (std::f64::consts::TAU * 55.0 * t).sin()) as f32 + 0.01 * noise();
        }
        out
    }

    fn tempo_of(samples: &[f32]) -> Option<f64> {
        let mut tracker = TempoTracker::new(RATE);
        for &sample in samples {
            tracker.push(sample);
        }
        tracker.finish()
    }

    #[test]
    fn known_tempos_are_found_within_two_percent() {
        // Outside about 70 to 150 BPM, a pattern repeating every two beats
        // (a backbeat) can have tracks measured at half or double tempo:
        // the tempo is corrected by hand then.
        for bpm in [72.0, 85.0, 90.0, 100.0, 110.0, 120.0, 128.0, 140.0, 150.0] {
            let found = tempo_of(&drums(bpm, 30.0)).unwrap();
            let error = (found - bpm).abs() / bpm;
            assert!(error < 0.02, "{bpm} BPM measured as {found}");
        }
    }

    #[test]
    fn short_or_beatless_audio_has_no_tempo() {
        assert_eq!(tempo_of(&drums(120.0, 5.0)), None);
        let tone: Vec<f32> = (0..RATE as usize * 20)
            .map(|n| {
                (0.3 * (std::f64::consts::TAU * 440.0 * n as f64 / f64::from(RATE)).sin()) as f32
            })
            .collect();
        assert_eq!(tempo_of(&tone), None);
        assert_eq!(tempo_of(&vec![0.0; RATE as usize * 20]), None);
        // A tone between silences, with some noise as lossy codecs add:
        // onsets only where it starts, and noise.
        let mut state = 0x9e37_79b9_u32;
        // The frequencies of scripts/generate-test-library.sh.
        for freq in (1..=24).map(|i| f64::from(220 + (i * 37) % 660)) {
            let gated: Vec<f32> = (0..RATE as usize * 25)
                .map(|n| {
                    let t = n as f64 / f64::from(RATE);
                    state ^= state << 13;
                    state ^= state >> 17;
                    state ^= state << 5;
                    let noise = (state as f32 / u32::MAX as f32 - 0.5) * 0.002;
                    if (2.0..22.0).contains(&t) {
                        (0.3 * (std::f64::consts::TAU * freq * t).sin()) as f32 + noise
                    } else {
                        0.0
                    }
                })
                .collect();
            assert_eq!(tempo_of(&gated), None, "{freq} Hz");
        }
    }
}
