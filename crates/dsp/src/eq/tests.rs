// SPDX-License-Identifier: AGPL-3.0-or-later

use super::*;

const RATE: u32 = 48_000;

/// Stereo sine, both channels alike, starting at `phase` samples.
fn sine(frequency: f64, amplitude: f64, start: usize, frames: usize) -> Vec<f32> {
    (start..start + frames)
        .flat_map(|n| {
            let value = (amplitude * (TAU * frequency * n as f64 / f64::from(RATE)).sin()) as f32;
            [value, value]
        })
        .collect()
}

/// Amplitude of `frequency` in the left channel, over a whole number of periods.
fn amplitude(samples: &[f32], frequency: f64) -> f64 {
    let (mut sin, mut cos) = (0.0, 0.0);
    let frames = samples.len() / 2;
    for (n, frame) in samples.as_chunks::<2>().0.iter().enumerate() {
        let phase = TAU * frequency * n as f64 / f64::from(RATE);
        sin += f64::from(frame[0]) * phase.sin();
        cos += f64::from(frame[0]) * phase.cos();
    }
    2.0 * sin.hypot(cos) / frames as f64
}

/// Gain of the processed EQ at `frequency`, measured on a sine, in dB.
fn measured_db(gains: [f32; BANDS], frequency: f64) -> f64 {
    // 0.25 s to settle, then 0.25 s measured: frequencies are multiples of
    // 4 Hz, so the measure holds whole periods.
    const HALF: usize = RATE as usize / 4;
    let mut eq = GraphicEq::new(RATE);
    eq.restart(true, gains);
    let mut samples = sine(frequency, 0.25, 0, 2 * HALF);
    eq.process(&mut samples);
    20.0 * (amplitude(&samples[HALF * 2..], frequency) / 0.25).log10()
}

/// 20 Hz to 20 kHz, about three points per octave, multiples of 4 Hz.
fn test_frequencies() -> Vec<f64> {
    (0..=30)
        .map(|step| {
            let frequency = 20.0 * 1000f64.powf(f64::from(step) / 30.0);
            (frequency / 4.0).round() * 4.0
        })
        .collect()
}

/// Deterministic pseudo-random gains.
fn random_gains(seed: &mut u64) -> [f32; BANDS] {
    std::array::from_fn(|_| {
        *seed = seed
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        let unit = (*seed >> 40) as f32 / (1u64 << 24) as f32;
        (unit * 2.0 - 1.0) * MAX_GAIN_DB
    })
}

#[test]
fn a_band_alone_peaks_at_its_gain() {
    for (band, &centre) in FREQUENCIES.iter().enumerate() {
        for gain in [-12.0, -6.0, 3.0, 12.0] {
            let coefficients = Coefficients::peaking(f64::from(RATE), centre, Q, gain);
            let at_centre = coefficients.response_db(f64::from(RATE), centre);
            assert!(
                (at_centre - gain).abs() < 1e-9,
                "band {band}: {at_centre} dB"
            );
            // Far from it, nothing.
            let far = if centre < 1_000.0 { 20_000.0 } else { 20.0 };
            assert!(coefficients.response_db(f64::from(RATE), far).abs() < 0.5);
        }
    }
}

#[test]
fn flat_is_zero_everywhere() {
    for frequency in test_frequencies() {
        assert!(response_db(f64::from(RATE), &[0.0; BANDS], frequency).abs() < 1e-9);
    }
}

#[test]
fn the_processed_response_matches_the_design() {
    let mut settings = Vec::new();
    for band in 0..BANDS {
        for gain in [-MAX_GAIN_DB, MAX_GAIN_DB] {
            let mut gains = [0.0; BANDS];
            gains[band] = gain;
            settings.push(gains);
        }
    }
    let mut seed = 9;
    settings.extend((0..3).map(|_| random_gains(&mut seed)));

    for gains in settings {
        for frequency in test_frequencies() {
            let expected = response_db(f64::from(RATE), &gains, frequency);
            let measured = measured_db(gains, frequency);
            assert!(
                (measured - expected).abs() < 0.1,
                "{gains:?} at {frequency} Hz: {measured:.3} dB, expected {expected:.3} dB"
            );
        }
    }
}

#[test]
fn a_bypassed_eq_is_bit_identical() {
    let input = sine(440.0, 0.5, 0, 4_800);
    let mut eq = GraphicEq::new(RATE);
    let mut output = input.clone();
    eq.process(&mut output);
    assert_eq!(output, input);

    // Bypassed after playing: once the glide to flat is over.
    eq.set(true, [6.0; BANDS]);
    eq.process(&mut sine(440.0, 0.5, 0, 4_800));
    eq.set(false, [6.0; BANDS]);
    eq.process(&mut sine(440.0, 0.5, 0, 9_600));
    assert!(!eq.is_active());
    let mut output = input.clone();
    eq.process(&mut output);
    assert_eq!(output, input);
}

#[test]
fn a_flat_eq_changes_nothing() {
    let input = sine(440.0, 0.5, 0, 4_800);
    let mut eq = GraphicEq::new(RATE);
    eq.restart(true, [0.0; BANDS]);
    let mut output = input.clone();
    eq.process(&mut output);
    for (out, original) in output.iter().zip(&input) {
        assert!((out - original).abs() < 1e-6);
    }
}

#[test]
fn gains_are_clamped() {
    let mut gains = [0.0; BANDS];
    gains[0] = 40.0;
    gains[1] = -40.0;
    gains[2] = f32::NAN;
    let clamped = clamp_gains(gains);
    assert_eq!(&clamped[..3], &[12.0, -12.0, 0.0]);
}

#[test]
fn gains_glide_in_about_50_ms() {
    let mut eq = GraphicEq::new(RATE);
    eq.restart(true, [-12.0; BANDS]);
    eq.set(true, [12.0; BANDS]);
    // After 10 ms, still on the way.
    eq.process(&mut sine(1_000.0, 0.1, 0, 480));
    assert!(eq.current_db.iter().all(|gain| *gain < 6.0));
    // After 50 ms, within 1 dB.
    eq.process(&mut sine(1_000.0, 0.1, 480, 1_920));
    assert!(eq.current_db.iter().all(|gain| *gain > 11.0));
    // After 250 ms, there.
    eq.process(&mut sine(1_000.0, 0.1, 2_400, 9_600));
    assert_eq!(eq.current_db, [12.0; BANDS]);
    assert!(!eq.gliding);
}

#[test]
fn a_restart_takes_the_gains_at_once() {
    let mut eq = GraphicEq::new(RATE);
    eq.restart(true, [5.0; BANDS]);
    assert_eq!(eq.current_db, [5.0; BANDS]);
    assert!(eq.is_active());
    eq.restart(false, [5.0; BANDS]);
    assert!(!eq.is_active());
}

#[test]
fn silence_leaves_no_denormals() {
    let mut eq = GraphicEq::new(RATE);
    eq.restart(true, [12.0; BANDS]);
    let mut impulse = vec![0.0; 2 * 480];
    impulse[0] = 1.0;
    impulse[1] = 1.0;
    eq.process(&mut impulse);
    for _ in 0..1_000 {
        eq.process(&mut [0.0; 2 * 480]);
    }
    assert!(
        eq.state
            .as_flattened()
            .as_flattened()
            .iter()
            .all(|value| *value == 0.0)
    );
}

/// Largest step between two samples of the left channel.
fn steepness(samples: &[f32]) -> f64 {
    samples
        .as_chunks::<2>()
        .0
        .iter()
        .map(|frame| f64::from(frame[0]))
        .collect::<Vec<_>>()
        .windows(2)
        .map(|pair| (pair[1] - pair[0]).abs())
        .fold(0.0, f64::max)
}

/// The steepness of a steady sine at `frequency` with the louder of the
/// settings, a little over: a click-free change stays under it.
fn steady_steepness(frequency: f64, settings: &[[f32; BANDS]]) -> f64 {
    let loudest = settings
        .iter()
        .map(|gains| response_db(f64::from(RATE), gains, frequency))
        .fold(f64::MIN, f64::max);
    let amplitude = 0.25 * 10f64.powf(loudest / 20.0);
    let w = TAU * frequency / f64::from(RATE);
    amplitude * 2.0 * (w / 2.0).sin() * 1.02
}

/// Energy of the left channel farther than 500 Hz from `frequency`,
/// relative to the whole, in dB (Hann window, DFT bins up to 20 kHz).
fn energy_away_db(samples: &[f32], frequency: f64) -> f64 {
    let left: Vec<f64> = samples
        .as_chunks::<2>()
        .0
        .iter()
        .map(|f| f64::from(f[0]))
        .collect();
    let n = left.len();
    let windowed: Vec<f64> = left
        .iter()
        .enumerate()
        .map(|(i, x)| x * (0.5 - 0.5 * (TAU * i as f64 / n as f64).cos()))
        .collect();
    let bin_hz = f64::from(RATE) / n as f64;
    let (mut away, mut total) = (0.0, 0.0);
    for bin in 1..(20_000.0 / bin_hz) as usize {
        let (mut re, mut im) = (0.0, 0.0);
        for (i, x) in windowed.iter().enumerate() {
            let phase = TAU * (bin * i) as f64 / n as f64;
            re += x * phase.cos();
            im -= x * phase.sin();
        }
        let power = re * re + im * im;
        total += power;
        if (bin as f64 * bin_hz - frequency).abs() > 500.0 {
            away += power;
        }
    }
    10.0 * (away.max(1e-300) / total).log10()
}

/// Frames kept before a change, and rendered after it: the change is in
/// the middle of what is analyzed.
const SIDE_FRAMES: usize = 4_096;
/// Frames rendered before the frames kept, for the filters to settle.
const SETTLE_FRAMES: usize = 9_600;

/// Renders a sine at `frequency` through `eq` in 10 ms blocks, near a peak
/// of the sine when `change` is called, and returns 85 ms before the change
/// and 85 ms after it.
fn render_change(
    eq: &mut GraphicEq,
    frequency: f64,
    change: impl FnOnce(&mut GraphicEq),
) -> Vec<f32> {
    let start = change_start(frequency) - SIDE_FRAMES;
    eq.process(&mut sine(
        frequency,
        0.25,
        start - SETTLE_FRAMES,
        SETTLE_FRAMES,
    ));
    let mut output = sine(frequency, 0.25, start, 2 * SIDE_FRAMES);
    let (before, after) = output.split_at_mut(2 * SIDE_FRAMES);
    for block in before.chunks_mut(2 * 480) {
        eq.process(block);
    }
    change(eq);
    for block in after.chunks_mut(2 * 480) {
        eq.process(block);
    }
    output
}

/// Frame of the sine where the change happens: a quarter period after a
/// whole number of periods, near a peak, the worst place for a step.
fn change_start(frequency: f64) -> usize {
    SETTLE_FRAMES + SIDE_FRAMES + (f64::from(RATE) / frequency / 4.0) as usize
}

/// Gain and phase of the EQ's design at `frequency`.
fn complex_response(gains: &[f64; BANDS], frequency: f64) -> (f64, f64) {
    let w = TAU * frequency / f64::from(RATE);
    let (c1, s1, c2, s2) = (w.cos(), w.sin(), (2.0 * w).cos(), (2.0 * w).sin());
    FREQUENCIES
        .iter()
        .zip(gains)
        .fold((1.0, 0.0), |(gain, phase), (&centre, &gain_db)| {
            let c = Coefficients::peaking(f64::from(RATE), centre, Q, gain_db);
            let (num_re, num_im) = (c.b0 + c.b1 * c1 + c.b2 * c2, -(c.b1 * s1 + c.b2 * s2));
            let (den_re, den_im) = (1.0 + c.a1 * c1 + c.a2 * c2, -(c.a1 * s1 + c.a2 * s2));
            (
                gain * num_re.hypot(num_im) / den_re.hypot(den_im),
                phase + num_im.atan2(num_re) - den_im.atan2(den_re),
            )
        })
}

/// What a click-free change sounds like: the sine of `render_change` with
/// the gain and phase of the design, gliding from `from` to `to` as the
/// gains glide (in steps every 32 frames, linear in between).
fn reference_glide(frequency: f64, from: &[f32; BANDS], to: &[f32; BANDS]) -> Vec<f32> {
    let smoothing = 1.0 - (-(UPDATE_FRAMES as f64) / (SMOOTHING_TAU_S * f64::from(RATE))).exp();
    let target = to.map(f64::from);
    let mut gains = from.map(f64::from);
    let mut previous = complex_response(&gains, frequency);
    let mut next = previous;
    let start = change_start(frequency) - SIDE_FRAMES;
    let mut output = Vec::with_capacity(4 * SIDE_FRAMES);
    for n in 0..2 * SIDE_FRAMES {
        let since_change = n.checked_sub(SIDE_FRAMES);
        if since_change.is_some_and(|frames| frames % UPDATE_FRAMES == 0) {
            previous = next;
            for (gain, target) in gains.iter_mut().zip(&target) {
                *gain += (target - *gain) * smoothing;
            }
            next = complex_response(&gains, frequency);
        }
        let t = since_change.map_or(0.0, |frames| {
            (frames % UPDATE_FRAMES + 1) as f64 / UPDATE_FRAMES as f64
        });
        let gain = previous.0 + (next.0 - previous.0) * t;
        let phase = previous.1 + (next.1 - previous.1) * t;
        let angle = TAU * frequency * (start + n) as f64 / f64::from(RATE) + phase;
        let value = (0.25 * gain * angle.sin()) as f32;
        output.extend([value, value]);
    }
    output
}

/// Renders the change from `from` to `to` as a jump: the coefficients
/// changed at once, without a glide. A click.
fn render_jump(frequency: f64, from: [f32; BANDS], to: [f32; BANDS]) -> Vec<f32> {
    let mut eq = GraphicEq::new(RATE);
    eq.restart(true, from);
    render_change(&mut eq, frequency, |eq| {
        eq.set(true, to);
        eq.restart_track_keeping_state();
    })
}

impl GraphicEq {
    /// The gains at their targets at once, the filters as they are.
    fn restart_track_keeping_state(&mut self) {
        let state = self.state;
        self.restart_track();
        self.state = state;
    }
}

/// Asserts that a change from `from` to `to`, rendered by `render_change`,
/// does not click:
/// - no step between two samples larger than the steady sine's;
/// - energy away from the sine at most 10 dB over the reference glide's (a
///   filter takes a few milliseconds to settle, the reference none) and at
///   least 30 dB under a jump's.
fn assert_smooth(output: &[f32], frequency: f64, from: [f32; BANDS], to: [f32; BANDS]) {
    let steep = steepness(output);
    let bound = steady_steepness(frequency, &[from, to]);
    assert!(
        steep <= bound,
        "{frequency} Hz: steps of {steep}, more than {bound}"
    );
    let away = energy_away_db(output, frequency);
    let reference = energy_away_db(&reference_glide(frequency, &from, &to), frequency);
    let jump = energy_away_db(&render_jump(frequency, from, to), frequency);
    assert!(
        away < reference + 10.0 && away < jump - 30.0,
        "{frequency} Hz: {away:.1} dB away from the sine, the reference {reference:.1} dB, a jump {jump:.1} dB"
    );
}

fn band_alone(band: usize, gain: f32) -> [f32; BANDS] {
    let mut gains = [0.0; BANDS];
    gains[band] = gain;
    gains
}

#[test]
fn moving_a_slider_does_not_click() {
    // 170 Hz band, from −12 to +12 dB and back, on a 170 Hz sine.
    let (low, high) = (band_alone(1, -12.0), band_alone(1, 12.0));
    for (from, to) in [(low, high), (high, low)] {
        let mut eq = GraphicEq::new(RATE);
        eq.restart(true, from);
        let output = render_change(&mut eq, 170.0, |eq| eq.set(true, to));
        assert_smooth(&output, 170.0, from, to);
    }
}

#[test]
fn an_unsmoothed_jump_would_click() {
    // The checks can tell: a jump clicks.
    let (low, high) = (band_alone(1, -12.0), band_alone(1, 12.0));
    let jump = render_jump(170.0, low, high);
    assert!(steepness(&jump) > steady_steepness(170.0, &[low, high]) * 2.0);
    let reference = energy_away_db(&reference_glide(170.0, &low, &high), 170.0);
    let away = energy_away_db(&jump, 170.0);
    assert!(
        away > reference + 20.0,
        "{away:.1} dB, the reference {reference:.1} dB"
    );
}

#[test]
fn loading_a_preset_does_not_click() {
    // Every band up, so the level at any frequency rises steadily.
    let before = [-9.0, -6.0, -3.0, -12.0, -6.0, -2.0, -8.0, -4.0, -6.0, -1.0];
    let after = [6.0, 9.0, 3.0, 12.0, 2.0, 5.0, 0.0, 4.0, 6.0, 8.0];
    for frequency in [60.0, 300.0, 1_000.0, 5_000.0] {
        let mut eq = GraphicEq::new(RATE);
        eq.restart(true, before);
        let output = render_change(&mut eq, frequency, |eq| eq.set(true, after));
        assert_smooth(&output, frequency, before, after);
    }
}

#[test]
fn enabling_and_bypassing_do_not_click() {
    let gains = band_alone(1, 12.0);
    let mut eq = GraphicEq::new(RATE);
    let enabled = render_change(&mut eq, 170.0, |eq| eq.set(true, gains));
    assert_smooth(&enabled, 170.0, [0.0; BANDS], gains);
    let mut eq = GraphicEq::new(RATE);
    eq.restart(true, gains);
    let bypassed = render_change(&mut eq, 170.0, |eq| eq.set(false, gains));
    assert_smooth(&bypassed, 170.0, gains, [0.0; BANDS]);
    // Bypassed once the glide is over.
    eq.process(&mut sine(170.0, 0.25, 0, 9_600));
    assert!(!eq.is_active());
}
