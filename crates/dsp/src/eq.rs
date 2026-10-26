// SPDX-License-Identifier: AGPL-3.0-or-later

//! The 10-band graphic EQ of the decks.
//!
//! Ten peaking filters in series, one octave wide, ±12 dB: the peaking EQ of
//! Robert Bristow-Johnson's *Audio EQ Cookbook* ([`Coefficients`], the design),
//! run as state-variable filters ([`Bell`]), which change gain smoothly. Gains glide to new settings
//! instead of stepping, and a bypassed EQ is not computed at all.
//!
//! [`GraphicEq::process`] runs on the audio thread: it never allocates.
//! See `docs/audio-engine.md#deck-eq`.

use std::f64::consts::{SQRT_2, TAU};

/// Number of bands.
pub const BANDS: usize = 10;
/// Centre frequency of each band, in Hz.
pub const FREQUENCIES: [f64; BANDS] = [
    60.0, 170.0, 310.0, 600.0, 1_000.0, 3_000.0, 6_000.0, 12_000.0, 14_000.0, 16_000.0,
];
/// Highest boost and deepest cut of a band, in dB.
pub const MAX_GAIN_DB: f32 = 12.0;
/// Quality factor of every band: one octave.
pub const Q: f64 = SQRT_2;
/// Interleaved stereo.
const CHANNELS: usize = 2;
/// Coefficients are recomputed this often while gains glide (0.7 ms at
/// 48 kHz), and interpolated sample by sample in between.
const UPDATE_FRAMES: usize = 32;
/// Time constant of the glide to new gains: there in about 50 ms.
const SMOOTHING_TAU_S: f64 = 0.015;
/// A glide ends when every band is this close to its target.
const SETTLED_DB: f64 = 0.001;
/// Filter state below this is flushed to zero, so silence never leaves
/// denormal numbers in the recursion (slow on some CPUs).
const DENORMAL_FLOOR: f64 = 1e-150;

/// Coefficients of one biquad, normalized (`a0` = 1).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Coefficients {
    pub b0: f64,
    pub b1: f64,
    pub b2: f64,
    pub a1: f64,
    pub a2: f64,
}

impl Coefficients {
    /// A peaking filter: `gain_db` at `frequency`, 0 dB far from it.
    pub fn peaking(sample_rate: f64, frequency: f64, q: f64, gain_db: f64) -> Self {
        let w0 = TAU * frequency / sample_rate;
        let (cos_w0, alpha) = (w0.cos(), w0.sin() / (2.0 * q));
        let a = 10f64.powf(gain_db / 40.0);
        let a0 = 1.0 + alpha / a;
        Self {
            b0: (1.0 + alpha * a) / a0,
            b1: -2.0 * cos_w0 / a0,
            b2: (1.0 - alpha * a) / a0,
            a1: -2.0 * cos_w0 / a0,
            a2: (1.0 - alpha / a) / a0,
        }
    }

    /// Gain of the filter at `frequency`, in dB.
    pub fn response_db(&self, sample_rate: f64, frequency: f64) -> f64 {
        let w = TAU * frequency / sample_rate;
        // |H(e^jw)|² with z^-1 = e^-jw.
        let (c1, s1, c2, s2) = (w.cos(), w.sin(), (2.0 * w).cos(), (2.0 * w).sin());
        let num_re = self.b0 + self.b1 * c1 + self.b2 * c2;
        let num_im = -(self.b1 * s1 + self.b2 * s2);
        let den_re = 1.0 + self.a1 * c1 + self.a2 * c2;
        let den_im = -(self.a1 * s1 + self.a2 * s2);
        10.0 * ((num_re * num_re + num_im * num_im) / (den_re * den_re + den_im * den_im)).log10()
    }
}

/// Gain of the whole EQ at `frequency`, in dB: the sum of its bands'.
/// What the EQ's response curve shows.
pub fn response_db(sample_rate: f64, gains_db: &[f32; BANDS], frequency: f64) -> f64 {
    FREQUENCIES
        .iter()
        .zip(gains_db)
        .map(|(&centre, &gain)| {
            Coefficients::peaking(sample_rate, centre, Q, f64::from(gain))
                .response_db(sample_rate, frequency)
        })
        .sum()
}

/// Clamps gains to ±[`MAX_GAIN_DB`]; NaN becomes 0 dB.
pub fn clamp_gains(gains_db: [f32; BANDS]) -> [f32; BANDS] {
    gains_db.map(|gain| {
        if gain.is_nan() {
            0.0
        } else {
            gain.clamp(-MAX_GAIN_DB, MAX_GAIN_DB)
        }
    })
}

/// One band as a trapezoidal state-variable filter (Andrew Simper,
/// *Linear Trap Optimised SVF*). Its response is exactly that of
/// [`Coefficients::peaking`], and unlike a direct form biquad its state stays
/// meaningful when the gain changes while it plays: no click on a glide.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Bell {
    a1: f64,
    a2: f64,
    a3: f64,
    /// Gain of the band-pass output added to the input: k (A² − 1).
    m1: f64,
}

impl Bell {
    /// `g` is tan(π f / fs), which does not depend on the gain.
    fn new(g: f64, gain_db: f64) -> Self {
        let a = 10f64.powf(gain_db / 40.0);
        let k = 1.0 / (Q * a);
        let a1 = 1.0 / (1.0 + g * (g + k));
        let a2 = g * a1;
        Self {
            a1,
            a2,
            a3: g * a2,
            m1: k * (a * a - 1.0),
        }
    }

    const ZERO: Self = Self {
        a1: 0.0,
        a2: 0.0,
        a3: 0.0,
        m1: 0.0,
    };

    /// What to add at each of `frames` samples to go from `self` to `to`.
    fn step_to(&self, to: &Self, frames: usize) -> Self {
        let n = frames as f64;
        Self {
            a1: (to.a1 - self.a1) / n,
            a2: (to.a2 - self.a2) / n,
            a3: (to.a3 - self.a3) / n,
            m1: (to.m1 - self.m1) / n,
        }
    }

    fn add(&mut self, step: &Self) {
        self.a1 += step.a1;
        self.a2 += step.a2;
        self.a3 += step.a3;
        self.m1 += step.m1;
    }
}

/// The 10-band EQ of one deck, on interleaved stereo.
pub struct GraphicEq {
    /// Per band: tan(π f / fs).
    warped: [f64; BANDS],
    bells: [Bell; BANDS],
    /// Added to `bells` at each sample while gliding.
    steps: [Bell; BANDS],
    /// The two integrators of each band, per channel.
    state: [[[f64; 2]; CHANNELS]; BANDS],
    current_db: [f64; BANDS],
    target_db: [f64; BANDS],
    /// The EQ is wanted (it may still be gliding to flat when not).
    enabled: bool,
    /// The EQ is computed: enabled, or gliding to flat before the bypass.
    active: bool,
    gliding: bool,
    /// The gains reached their targets: the coefficients get there at the
    /// end of this update.
    last_update: bool,
    /// Frames until the coefficients are next recomputed.
    until_update: usize,
    /// Share of the way to the target covered at each update.
    smoothing: f64,
}

impl GraphicEq {
    /// A bypassed, flat EQ.
    pub fn new(sample_rate: u32) -> Self {
        let rate = f64::from(sample_rate);
        let warped = FREQUENCIES.map(|frequency| (std::f64::consts::PI * frequency / rate).tan());
        let update_s = UPDATE_FRAMES as f64 / rate;
        Self {
            warped,
            bells: warped.map(|g| Bell::new(g, 0.0)),
            steps: [Bell::ZERO; BANDS],
            state: [[[0.0; 2]; CHANNELS]; BANDS],
            current_db: [0.0; BANDS],
            target_db: [0.0; BANDS],
            enabled: false,
            active: false,
            gliding: false,
            last_update: false,
            until_update: 0,
            smoothing: 1.0 - (-update_s / SMOOTHING_TAU_S).exp(),
        }
    }

    /// New settings, reached with a glide. Enabling glides from flat;
    /// bypassing glides to flat, then stops computing the EQ.
    pub fn set(&mut self, enabled: bool, gains_db: [f32; BANDS]) {
        if enabled && !self.active {
            self.state = [[[0.0; 2]; CHANNELS]; BANDS];
            self.current_db = [0.0; BANDS];
            self.bells = self.bells_at(&self.current_db);
            self.active = true;
        }
        self.enabled = enabled;
        self.target_db = if enabled {
            clamp_gains(gains_db).map(f64::from)
        } else {
            [0.0; BANDS]
        };
        self.gliding = self.active;
        self.last_update = false;
        self.until_update = 0;
    }

    /// New settings at once, without a glide, and the filters at rest: for
    /// an engine starting.
    pub fn restart(&mut self, enabled: bool, gains_db: [f32; BANDS]) {
        self.set(enabled, gains_db);
        self.restart_track();
    }

    /// The filters at rest and the gains at their targets, without a glide:
    /// a new track starts on the deck.
    pub fn restart_track(&mut self) {
        self.state = [[[0.0; 2]; CHANNELS]; BANDS];
        self.current_db = self.target_db;
        self.gliding = false;
        self.last_update = false;
        self.active = self.enabled;
        self.bells = self.bells_at(&self.current_db);
        self.steps = [Bell::ZERO; BANDS];
    }

    /// The EQ is computed (enabled, or gliding to its bypass).
    pub fn is_active(&self) -> bool {
        self.active
    }

    /// Filters interleaved stereo in place. Does nothing while bypassed.
    pub fn process(&mut self, samples: &mut [f32]) {
        if !self.active {
            return;
        }
        let whole = samples.len() / CHANNELS * CHANNELS;
        let mut frames = &mut samples[..whole];
        while !frames.is_empty() {
            if self.gliding && self.until_update == 0 {
                self.glide();
                if !self.active {
                    return;
                }
                self.until_update = UPDATE_FRAMES;
            }
            let length = if self.gliding {
                self.until_update.min(frames.len() / CHANNELS)
            } else {
                frames.len() / CHANNELS
            };
            let (chunk, rest) = frames.split_at_mut(length * CHANNELS);
            if self.gliding {
                self.filter::<true>(chunk);
            } else {
                self.filter::<false>(chunk);
            }
            if self.gliding {
                self.until_update -= length;
            }
            frames = rest;
        }
        self.flush_denormals();
    }

    fn filter<const GLIDING: bool>(&mut self, samples: &mut [f32]) {
        for frame in samples.as_chunks_mut::<CHANNELS>().0 {
            if GLIDING {
                for (bell, step) in self.bells.iter_mut().zip(&self.steps) {
                    bell.add(step);
                }
            }
            for (channel, sample) in frame.iter_mut().enumerate() {
                let mut x = f64::from(*sample);
                for (bell, state) in self.bells.iter().zip(&mut self.state) {
                    let [ic1, ic2] = &mut state[channel];
                    let v3 = x - *ic2;
                    let v1 = bell.a1 * *ic1 + bell.a2 * v3;
                    let v2 = *ic2 + bell.a2 * *ic1 + bell.a3 * v3;
                    *ic1 = 2.0 * v1 - *ic1;
                    *ic2 = 2.0 * v2 - *ic2;
                    x += bell.m1 * v1;
                }
                *sample = x as f32;
            }
        }
    }

    /// One step of the glide towards the targets: the coefficients move to
    /// the next gains over the next [`UPDATE_FRAMES`].
    fn glide(&mut self) {
        if self.last_update {
            // The coefficients are there: exactly, from here.
            self.gliding = false;
            self.last_update = false;
            self.bells = self.bells_at(&self.target_db);
            self.steps = [Bell::ZERO; BANDS];
            if !self.enabled {
                // Flat now: bypass from here.
                self.active = false;
                self.state = [[[0.0; 2]; CHANNELS]; BANDS];
            }
            return;
        }
        let mut settled = true;
        for (current, target) in self.current_db.iter_mut().zip(&self.target_db) {
            *current += (target - *current) * self.smoothing;
            if (target - *current).abs() < SETTLED_DB {
                *current = *target;
            } else {
                settled = false;
            }
        }
        self.last_update = settled;
        let next = self.bells_at(&self.current_db);
        for ((step, bell), next) in self.steps.iter_mut().zip(&self.bells).zip(&next) {
            *step = bell.step_to(next, UPDATE_FRAMES);
        }
    }

    fn bells_at(&self, gains_db: &[f64; BANDS]) -> [Bell; BANDS] {
        std::array::from_fn(|band| Bell::new(self.warped[band], gains_db[band]))
    }

    fn flush_denormals(&mut self) {
        for value in self.state.as_flattened_mut().as_flattened_mut() {
            if value.abs() < DENORMAL_FLOOR {
                *value = 0.0;
            }
        }
    }
}

#[cfg(test)]
mod tests;
