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

/// Gain of the processed EQ at `frequency` with these filter gains,
/// measured on a sine, in dB.
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

const FS: f64 = RATE as f64;

#[test]
fn each_band_has_its_shape() {
    for band in 0..BANDS {
        let centre = FREQUENCIES[band];
        for gain in [-12.0, -6.0, 3.0, 12.0] {
            let design = Coefficients::band(FS, band, gain);
            let at = |frequency| design.response_db(FS, frequency);
            match SHAPES[band] {
                Shape::Peaking => {
                    assert!((at(centre) - gain).abs() < 1e-9, "band {band}");
                    let far = if centre < 1_000.0 { 20_000.0 } else { 20.0 };
                    assert!(at(far).abs() < 0.5, "band {band}");
                }
                // Half the gain at the shelf's frequency, all of it on its
                // side, nothing on the other.
                // Half the gain at the shelf's midpoint, most of it at the
                // band's frequency, an octave out, nothing on the other side.
                Shape::LowShelf => {
                    assert!((at(design_frequency(band)) - gain / 2.0).abs() < 1e-6);
                    assert!(
                        (at(centre) - gain).abs() < gain.abs() * 0.25,
                        "{}",
                        at(centre)
                    );
                    assert!((at(10.0) - gain).abs() < 0.3);
                    assert!(at(2_000.0).abs() < 0.1);
                }
                Shape::HighShelf => {
                    assert!((at(design_frequency(band)) - gain / 2.0).abs() < 1e-6);
                    assert!(
                        (at(centre) - gain).abs() < gain.abs() * 0.25,
                        "{}",
                        at(centre)
                    );
                    assert!(at(500.0).abs() < 0.1);
                }
            }
        }
    }
}

#[test]
fn bands_are_as_wide_as_their_neighbours_are_far() {
    let qs: Vec<f64> = (0..BANDS).map(band_q).collect();
    // 3k sits 1.6 and 1 octave from its neighbours, 14k a quarter of one.
    assert!((qs[5] - 1.08).abs() < 0.01, "{qs:?}");
    assert!((qs[8] - 6.95).abs() < 0.01, "{qs:?}");
    assert!((qs[0] - FRAC_1_SQRT_2).abs() < 1e-12);
    assert!((qs[9] - FRAC_1_SQRT_2).abs() < 1e-12);
}

#[test]
fn flat_is_zero_everywhere() {
    assert_eq!(filter_gains(FS, &[0.0; BANDS]), [0.0; BANDS]);
    for frequency in test_frequencies() {
        assert!(response_db(FS, &[0.0; BANDS], frequency).abs() < 1e-9);
    }
}

#[test]
fn the_curve_meets_every_slider_at_its_band() {
    let mut settings: Vec<[f32; BANDS]> = (0..BANDS)
        .flat_map(|band| {
            [-MAX_GAIN_DB, MAX_GAIN_DB].map(|gain| {
                let mut sliders = [0.0; BANDS];
                sliders[band] = gain;
                sliders
            })
        })
        .collect();
    let mut seed = 21;
    settings.extend((0..1_000).map(|_| random_gains(&mut seed)));
    let mut limited = 0;
    for sliders in &settings {
        let gains = filter_gains(FS, sliders);
        // Sliders pulling hard in opposite directions on close bands need
        // filter gains past the limit: their curve is less exact.
        let at_limit = gains.iter().any(|gain| gain.abs() >= MAX_FILTER_GAIN_DB);
        limited += usize::from(at_limit);
        let tolerance = if at_limit { 3.5 } else { 0.2 };
        for (band, &centre) in FREQUENCIES.iter().enumerate() {
            let at = filters_response_db(FS, &gains, centre);
            let wanted = f64::from(sliders[band]);
            assert!(
                (at - wanted).abs() < tolerance,
                "{sliders:?}: {at:.2} dB at {centre} Hz, the slider {wanted}"
            );
        }
    }
    // About one random setting in fifty.
    assert!(
        limited * 20 < settings.len(),
        "{limited} settings reached the limit"
    );
}

#[test]
fn equal_sliders_give_a_nearly_flat_curve() {
    for gain in [-12.0, -6.0, 6.0, 12.0] {
        let gains = filter_gains(FS, &[gain; BANDS]);
        for step in 0..=300 {
            let frequency = 20.0 * 1_000f64.powf(f64::from(step) / 300.0);
            let at = filters_response_db(FS, &gains, frequency);
            // Within half the sliders' gain, between bands and out to the
            // ends of the range.
            assert!(
                (at - f64::from(gain)).abs() < f64::from(gain.abs()) / 2.0,
                "all at {gain} dB: {at:.2} dB at {frequency:.0} Hz"
            );
        }
    }
}

#[test]
fn the_curve_hardly_rises_above_the_highest_slider() {
    // With the shelves' midpoints at their bands, a slider of +5 at 60 Hz
    // took +9 at 20 Hz: the curve went well above every slider.
    let mut seed = 33;
    let mut overshoots: Vec<f64> = (0..300)
        .map(|_| {
            let sliders = random_gains(&mut seed);
            let highest = sliders.iter().copied().fold(0.0, f32::max);
            max_boost_db(FS, &filter_gains(FS, &sliders)) - f64::from(highest)
        })
        .collect();
    overshoots.sort_by(f64::total_cmp);
    let p90 = overshoots[overshoots.len() * 9 / 10];
    assert!(
        p90 < 2.0,
        "90 % of random settings overshoot by up to {p90:.2} dB"
    );
    // A preset's curve tops out at its highest slider.
    let rock = [5.0, 3.0, -1.5, -3.0, -1.5, 1.5, 3.5, 4.5, 4.5, 4.0];
    let top = max_boost_db(FS, &filter_gains(FS, &rock));
    assert!((top - 5.0).abs() < 0.5, "{top}");
}

#[test]
fn opposite_close_bands_stay_within_the_limit() {
    let sliders = [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 12.0, -12.0, 12.0];
    let gains = filter_gains(FS, &sliders);
    assert!(
        gains
            .iter()
            .all(|gain| gain.is_finite() && gain.abs() <= MAX_FILTER_GAIN_DB)
    );
}

#[test]
fn the_processed_response_matches_the_design() {
    let mut settings: Vec<[f64; BANDS]> = Vec::new();
    for band in 0..BANDS {
        for gain in [-MAX_FILTER_GAIN_DB, -12.0, 12.0, MAX_FILTER_GAIN_DB] {
            let mut gains = [0.0; BANDS];
            gains[band] = gain;
            settings.push(gains);
        }
    }
    let mut seed = 9;
    settings.extend((0..3).map(|_| filter_gains(FS, &random_gains(&mut seed))));

    for gains in settings {
        let filters = gains.map(|gain| gain as f32);
        for frequency in test_frequencies() {
            let expected = filters_response_db(FS, &filters.map(f64::from), frequency);
            let measured = measured_db(filters, frequency);
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
        .map(|gains| filters_response_db(FS, &gains.map(f64::from), frequency))
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
    gains
        .iter()
        .enumerate()
        .fold((1.0, 0.0), |(gain, phase), (band, &gain_db)| {
            let c = Coefficients::band(FS, band, gain_db);
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

/// The web UI draws the EQ's curve with its own copy of the design
/// (`web/src/lib/eq.ts`): both are checked against this fixture. Write it
/// again with `HERON_UPDATE_FIXTURES=1 cargo test -p heron-dsp`.
const FIXTURE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../web/src/lib/eq.fixture.json"
);

fn fixture() -> serde_json::Value {
    let round = |value: f64| (value * 1e6).round() / 1e6;
    let mut seed = 5;
    let mut settings: Vec<[f32; BANDS]> = vec![
        [0.0; BANDS],
        [6.0; BANDS],
        [-12.0; BANDS],
        [0.0, 7.0, -8.0, 5.0, 7.5, 7.0, 6.0, 0.0, 0.0, 0.0],
        [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 12.0, -12.0, 12.0],
        [12.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, -12.0],
    ];
    settings.extend((0..2).map(|_| random_gains(&mut seed).map(|gain| (gain * 2.0).round() / 2.0)));
    let frequencies = test_frequencies();
    let cases: Vec<serde_json::Value> = settings
        .iter()
        .map(|sliders| {
            let gains = filter_gains(FS, sliders);
            serde_json::json!({
                "sliders_db": sliders,
                "filter_gains_db": gains.map(round),
                "response_db": frequencies
                    .iter()
                    .map(|&frequency| [frequency, round(filters_response_db(FS, &gains, frequency))])
                    .collect::<Vec<_>>(),
            })
        })
        .collect();
    serde_json::json!({ "sample_rate": FS, "cases": cases })
}

#[test]
fn the_web_fixture_is_current() {
    let current = fixture();
    if std::env::var_os("HERON_UPDATE_FIXTURES").is_some() {
        let mut json = serde_json::to_string_pretty(&current).unwrap();
        json.push('\n');
        std::fs::write(FIXTURE, json).unwrap();
        return;
    }
    let stored: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(FIXTURE)
            .expect("the fixture: write it with HERON_UPDATE_FIXTURES=1"),
    )
    .unwrap();
    assert!(
        close(&stored, &current),
        "web/src/lib/eq.fixture.json is out of date: write it again with HERON_UPDATE_FIXTURES=1"
    );
}

/// Equal, numbers within 1e-5: the last bits of `cos` or `log10` may
/// differ from one CPU to another.
fn close(a: &serde_json::Value, b: &serde_json::Value) -> bool {
    use serde_json::Value;
    match (a, b) {
        (Value::Number(a), Value::Number(b)) => {
            (a.as_f64().unwrap_or(f64::NAN) - b.as_f64().unwrap_or(f64::NAN)).abs() < 1e-5
        }
        (Value::Array(a), Value::Array(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(a, b)| close(a, b))
        }
        (Value::Object(a), Value::Object(b)) => {
            a.len() == b.len()
                && a.iter()
                    .all(|(key, a)| b.get(key).is_some_and(|b| close(a, b)))
        }
        (a, b) => a == b,
    }
}

#[test]
fn the_highest_boost_is_the_top_of_the_curve() {
    assert_eq!(max_boost_db(FS, &[0.0; BANDS]), 0.0);
    // Cuts only: no boost.
    assert_eq!(max_boost_db(FS, &[-6.0; BANDS]), 0.0);
    // One band: its gain, at its centre.
    let mut gains = [0.0; BANDS];
    gains[4] = 9.0;
    assert!((max_boost_db(FS, &gains) - 9.0).abs() < 0.1);
    // The sliders of a preset: about the highest slider, the curve meeting
    // them at their bands.
    let rock = filter_gains(FS, &[5.0, 3.0, -1.5, -3.0, -1.5, 1.5, 3.5, 4.5, 4.5, 4.0]);
    let boost = max_boost_db(FS, &rock);
    assert!((4.9..5.5).contains(&boost), "{boost}");
}
