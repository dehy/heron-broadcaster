// SPDX-License-Identifier: AGPL-3.0-or-later

//! Streaming analysis of decoded samples, and the detection of cue and fade
//! points from the resulting level envelope.

use ebur128::{EbuR128, Mode};
use heron_core::AnalysisSettings;

use crate::Error;

/// Length of one level frame, the time resolution of every detected point.
pub const FRAME_MS: f64 = 5.0;

/// True peaks above this mean the decoded audio is garbage.
const MAX_TRUE_PEAK_DB: f64 = 12.0;

/// Level used for digital silence, instead of minus infinity.
const SILENT_DB: f32 = -200.0;

/// What the analysis stores for a track. Times are in milliseconds from the
/// start of the decoded audio.
#[derive(Clone, Debug, PartialEq)]
pub struct Analysis {
    pub duration_ms: u32,
    /// EBU R128 integrated loudness. `None` for silent tracks.
    pub integrated_lufs: Option<f64>,
    /// Highest true peak across channels, in dBTP. `None` for silent tracks.
    pub true_peak_db: Option<f64>,
    /// Where playback starts (Gap Killer, start of track).
    pub cue_in_ms: u32,
    /// Where playback ends (Gap Killer, end of track or long internal silence).
    pub cue_out_ms: u32,
    /// Where the crossfade into the next track starts.
    pub fade_out_start_ms: Option<u32>,
    /// A silence longer than the configured maximum was found and the track
    /// is cut there (`cue_out_ms` is the start of that silence).
    pub has_internal_silence: bool,
}

impl Analysis {
    /// A track with no audible content: nothing to play.
    pub fn is_silent(&self) -> bool {
        self.cue_out_ms <= self.cue_in_ms
    }
}

/// Accumulates decoded audio and produces an [`Analysis`].
///
/// Memory use is independent of the track length apart from the level
/// envelope (4 bytes per 5 ms), so long mixes are fine.
pub struct Analyzer {
    rate: u32,
    channels: usize,
    loudness: EbuR128,
    /// Samples per channel in one level frame.
    frame_len: usize,
    sum_squares: f64,
    frame_fill: usize,
    levels: Vec<f32>,
    total_frames: u64,
}

impl Analyzer {
    pub fn new(rate: u32, channels: usize) -> Result<Self, Error> {
        let unsupported = Error::UnsupportedFormat { rate, channels };
        if rate < 8_000 || channels == 0 {
            return Err(unsupported);
        }
        let channel_count = u32::try_from(channels).map_err(|_| unsupported)?;
        let loudness = EbuR128::new(channel_count, rate, Mode::I | Mode::TRUE_PEAK)?;
        let frame_len = (f64::from(rate) * FRAME_MS / 1000.0).round() as usize;
        Ok(Self {
            rate,
            channels,
            loudness,
            frame_len,
            sum_squares: 0.0,
            frame_fill: 0,
            levels: Vec::new(),
            total_frames: 0,
        })
    }

    pub fn rate(&self) -> u32 {
        self.rate
    }

    pub fn channels(&self) -> usize {
        self.channels
    }

    /// Adds interleaved samples. A trailing partial frame is ignored.
    pub fn push(&mut self, interleaved: &[f32]) -> Result<(), Error> {
        let whole = interleaved.len() - interleaved.len() % self.channels;
        let interleaved = &interleaved[..whole];
        self.loudness.add_frames_f32(interleaved)?;
        let channels = self.channels as f64;
        for frame in interleaved.chunks_exact(self.channels) {
            let squares: f64 = frame.iter().map(|&s| f64::from(s) * f64::from(s)).sum();
            self.sum_squares += squares / channels;
            self.frame_fill += 1;
            if self.frame_fill == self.frame_len {
                self.close_level_frame();
            }
        }
        self.total_frames += (whole / self.channels) as u64;
        Ok(())
    }

    fn close_level_frame(&mut self) {
        let mean_square = self.sum_squares / self.frame_fill as f64;
        let level = if mean_square > 0.0 {
            (10.0 * mean_square.log10()) as f32
        } else {
            SILENT_DB
        };
        self.levels.push(level.max(SILENT_DB));
        self.sum_squares = 0.0;
        self.frame_fill = 0;
    }

    pub fn finish(mut self, settings: &AnalysisSettings) -> Result<Analysis, Error> {
        if self.total_frames == 0 {
            return Err(Error::Empty);
        }
        if self.frame_fill > 0 {
            self.close_level_frame();
        }
        let duration_ms = to_ms(self.total_frames, self.rate);

        let integrated = self.loudness.loudness_global()?;
        let integrated_lufs = integrated.is_finite().then_some(integrated);
        let mut peak = 0.0_f64;
        for channel in 0..self.channels {
            peak = peak.max(self.loudness.true_peak(channel as u32)?);
        }
        let true_peak_db = (peak > 0.0).then(|| 20.0 * peak.log10());
        // Real masters peak a few dB over full scale at most; far above that
        // the decoder produced garbage, and cue points would be nonsense.
        if let Some(true_peak_db) = true_peak_db.filter(|&db| db > MAX_TRUE_PEAK_DB) {
            return Err(Error::InvalidAudio { true_peak_db });
        }

        let frame_ms = self.frame_len as f64 * 1000.0 / f64::from(self.rate);
        let points = match integrated_lufs {
            Some(reference) => detect(&self.levels, reference, frame_ms, duration_ms, settings),
            None => Points::default(),
        };
        Ok(Analysis {
            duration_ms,
            integrated_lufs,
            true_peak_db,
            cue_in_ms: points.cue_in_ms,
            cue_out_ms: points.cue_out_ms,
            fade_out_start_ms: points.fade_out_start_ms,
            has_internal_silence: points.has_internal_silence,
        })
    }
}

fn to_ms(frames: u64, rate: u32) -> u32 {
    u32::try_from(frames.saturating_mul(1000) / u64::from(rate)).unwrap_or(u32::MAX)
}

#[derive(Debug, Default, PartialEq)]
struct Points {
    cue_in_ms: u32,
    cue_out_ms: u32,
    fade_out_start_ms: Option<u32>,
    has_internal_silence: bool,
}

/// Finds cue and fade points in a level envelope (dBFS, one value per
/// frame of `frame_ms`). Thresholds are relative to `reference` (the
/// integrated loudness).
fn detect(
    levels: &[f32],
    reference: f64,
    frame_ms: f64,
    duration_ms: u32,
    settings: &AnalysisSettings,
) -> Points {
    // The last frame can be partial: never point past the end of the audio.
    let ms = |frame: usize| ((frame as f64 * frame_ms).round() as u32).min(duration_ms);
    let frames = |duration_ms: u32| (f64::from(duration_ms) / frame_ms).floor() as usize;
    // A blip covers at most one more frame than its length, depending on alignment.
    let blip_frames = frames(settings.blip_max_ms) + 1;

    // Silence must be quiet relative to the track *and* in absolute terms,
    // so the soft intro of a loud master is not mistaken for silence.
    let silence = |relative: f64| (reference + relative).min(settings.silence_ceiling_dbfs);
    let start = sound_mask(levels, silence(settings.start_threshold_db), blip_frames);
    let end = sound_mask(levels, silence(settings.end_threshold_db), blip_frames);
    let Some(cue_in) = start.iter().position(|&sound| sound) else {
        return Points::default();
    };
    let Some(last_sound) = end.iter().rposition(|&sound| sound) else {
        return Points::default();
    };
    let mut cue_out = (last_sound + 1).max(cue_in);

    // A long silence inside the track ends it there (hidden tracks, long gaps).
    let max_silence = frames(settings.max_internal_silence_ms).max(1);
    let mut has_internal_silence = false;
    let mut silent_run = 0;
    for (index, &sound) in end.iter().enumerate().take(cue_out).skip(cue_in) {
        if sound {
            silent_run = 0;
            continue;
        }
        silent_run += 1;
        if silent_run >= max_silence {
            cue_out = index + 1 - silent_run;
            has_internal_silence = true;
            break;
        }
    }

    // Crossfade point: after the last loud moment, kept between
    // fade_min_ms and fade_max_ms before the cue-out.
    let fade = sound_mask(levels, reference + settings.fade_trigger_db, blip_frames);
    let trigger = fade[cue_in..cue_out]
        .iter()
        .rposition(|&loud| loud)
        .map_or(cue_in, |index| cue_in + index + 1);

    let (cue_in_ms, cue_out_ms, trigger_ms) = (ms(cue_in), ms(cue_out), ms(trigger));
    let earliest = cue_out_ms
        .saturating_sub(settings.fade_max_ms)
        .max(cue_in_ms);
    let latest = cue_out_ms
        .saturating_sub(settings.fade_min_ms)
        .max(earliest);
    Points {
        cue_in_ms,
        cue_out_ms,
        fade_out_start_ms: Some(trigger_ms.clamp(earliest, latest)),
        has_internal_silence,
    }
}

/// `true` for frames at or above `threshold`, with isolated bursts of at
/// most `blip_frames` treated as silence.
fn sound_mask(levels: &[f32], threshold: f64, blip_frames: usize) -> Vec<bool> {
    let mut mask: Vec<bool> = levels
        .iter()
        .map(|&level| f64::from(level) >= threshold)
        .collect();
    let mut index = 0;
    while index < mask.len() {
        if !mask[index] {
            index += 1;
            continue;
        }
        let run_start = index;
        while index < mask.len() && mask[index] {
            index += 1;
        }
        if index - run_start <= blip_frames {
            mask[run_start..index].fill(false);
        }
    }
    mask
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 48_000;

    /// Builds a stereo test signal from segments.
    #[derive(Default)]
    struct Signal(Vec<f32>);

    impl Signal {
        fn sine(mut self, seconds: f64, freq: f64, level_dbfs: f64) -> Self {
            let amplitude = 10_f64.powf(level_dbfs / 20.0);
            let start = self.0.len() / 2;
            for n in 0..(seconds * f64::from(RATE)) as usize {
                let t = (start + n) as f64 / f64::from(RATE);
                let sample = (amplitude * (std::f64::consts::TAU * freq * t).sin()) as f32;
                self.0.extend([sample, sample]);
            }
            self
        }

        fn tone(self, seconds: f64, level_dbfs: f64) -> Self {
            self.sine(seconds, 1000.0, level_dbfs)
        }

        fn silence(mut self, seconds: f64) -> Self {
            let samples = (seconds * f64::from(RATE)) as usize * 2;
            self.0.extend(std::iter::repeat_n(0.0, samples));
            self
        }

        fn analyze(&self) -> Analysis {
            self.analyze_with(&AnalysisSettings::default())
        }

        fn analyze_with(&self, settings: &AnalysisSettings) -> Analysis {
            let mut analyzer = Analyzer::new(RATE, 2).unwrap();
            // Feed in uneven chunks, like a decoder would.
            for chunk in self.0.chunks(2 * 1_152) {
                analyzer.push(chunk).unwrap();
            }
            analyzer.finish(settings).unwrap()
        }
    }

    fn assert_near(actual: u32, expected: u32, tolerance: u32, what: &str) {
        assert!(
            actual.abs_diff(expected) <= tolerance,
            "{what}: expected {expected} ± {tolerance} ms, got {actual} ms"
        );
    }

    #[test]
    fn measures_loudness_and_true_peak() {
        // EBU Tech 3341: a stereo 1 kHz sine at -23 dBFS measures -23 LUFS.
        let analysis = Signal::default().tone(20.0, -23.0).analyze();
        let lufs = analysis.integrated_lufs.unwrap();
        assert!((lufs + 23.0).abs() < 0.2, "integrated loudness {lufs}");
        let peak = analysis.true_peak_db.unwrap();
        assert!((peak + 23.0).abs() < 0.3, "true peak {peak}");
        assert_eq!(analysis.duration_ms, 20_000);
    }

    #[test]
    fn trims_silence_at_start_and_end() {
        let analysis = Signal::default()
            .silence(2.0)
            .tone(10.0, -10.0)
            .silence(3.0)
            .analyze();
        assert_near(analysis.cue_in_ms, 2_000, 5, "cue-in");
        assert_near(analysis.cue_out_ms, 12_000, 5, "cue-out");
        assert!(!analysis.has_internal_silence);
        assert_eq!(analysis.duration_ms, 15_000);
    }

    #[test]
    fn ignores_blips_before_the_music() {
        let analysis = Signal::default()
            .silence(1.0)
            .tone(0.008, -6.0)
            .silence(1.0)
            .tone(10.0, -10.0)
            .analyze();
        assert_near(analysis.cue_in_ms, 2_008, 5, "cue-in");
    }

    #[test]
    fn ignores_blips_after_the_music() {
        let analysis = Signal::default()
            .tone(10.0, -10.0)
            .silence(1.0)
            .tone(0.008, -6.0)
            .silence(1.0)
            .analyze();
        assert_near(analysis.cue_out_ms, 10_000, 5, "cue-out");
    }

    #[test]
    fn keeps_short_sounds_longer_than_a_blip() {
        let analysis = Signal::default()
            .silence(1.0)
            .tone(0.050, -6.0)
            .silence(0.5)
            .tone(10.0, -10.0)
            .analyze();
        assert_near(analysis.cue_in_ms, 1_000, 5, "cue-in");
    }

    #[test]
    fn crossfade_starts_where_a_short_quiet_outro_begins() {
        // 1 s outro 20 dB below the music: under the -15 dB trigger, above
        // the -24 dB end threshold, so it is played but faded over.
        let analysis = Signal::default()
            .tone(10.0, -10.0)
            .tone(1.0, -30.0)
            .silence(1.0)
            .analyze();
        assert_near(analysis.cue_out_ms, 11_000, 5, "cue-out");
        assert_near(analysis.fade_out_start_ms.unwrap(), 10_000, 5, "fade start");
    }

    #[test]
    fn crossfade_point_is_at_most_fade_max_before_the_end() {
        let analysis = Signal::default()
            .tone(10.0, -10.0)
            .tone(5.0, -30.0)
            .analyze();
        assert_near(analysis.cue_out_ms, 15_000, 5, "cue-out");
        assert_near(analysis.fade_out_start_ms.unwrap(), 12_500, 5, "fade start");
    }

    #[test]
    fn crossfade_point_is_at_least_fade_min_before_the_end() {
        let analysis = Signal::default().tone(10.0, -10.0).silence(1.0).analyze();
        assert_near(
            analysis.fade_out_start_ms.unwrap(),
            10_000 - 150,
            5,
            "fade start",
        );
    }

    #[test]
    fn long_internal_silence_ends_the_track() {
        let analysis = Signal::default()
            .tone(5.0, -10.0)
            .silence(20.0)
            .tone(5.0, -10.0)
            .analyze();
        assert!(analysis.has_internal_silence);
        assert_near(analysis.cue_out_ms, 5_000, 5, "cue-out");
    }

    #[test]
    fn shorter_internal_silence_is_kept() {
        let analysis = Signal::default()
            .tone(5.0, -10.0)
            .silence(10.0)
            .tone(5.0, -10.0)
            .analyze();
        assert!(!analysis.has_internal_silence);
        assert_near(analysis.cue_out_ms, 20_000, 5, "cue-out");
    }

    #[test]
    fn thresholds_follow_the_track_loudness() {
        // The same quiet intro, 15 dB under the music, is kept on a quiet
        // master and on a loud one.
        for music_db in [-30.0, -8.0] {
            let analysis = Signal::default()
                .tone(2.0, music_db - 15.0)
                .tone(10.0, music_db)
                .analyze();
            assert_near(analysis.cue_in_ms, 0, 5, "cue-in");
        }
    }

    #[test]
    fn soft_intro_of_a_loud_master_is_not_trimmed() {
        // 10 s at -42 dBFS before music at -8 dBFS: 34 dB under the music,
        // far below the -20 dB relative threshold, but audible, so kept.
        let analysis = Signal::default()
            .silence(1.0)
            .tone(10.0, -42.0)
            .tone(10.0, -8.0)
            .analyze();
        assert_near(analysis.cue_in_ms, 1_000, 5, "cue-in");
    }

    #[test]
    fn inaudible_fade_in_is_trimmed() {
        let analysis = Signal::default()
            .tone(5.0, -70.0)
            .tone(10.0, -8.0)
            .analyze();
        assert_near(analysis.cue_in_ms, 5_000, 5, "cue-in");
    }

    #[test]
    fn silent_track_has_nothing_to_play() {
        let analysis = Signal::default().silence(5.0).analyze();
        assert!(analysis.is_silent());
        assert_eq!(analysis.integrated_lufs, None);
        assert_eq!(analysis.true_peak_db, None);
        assert_eq!(analysis.duration_ms, 5_000);
    }

    #[test]
    fn works_at_44_1_khz() {
        let mut analyzer = Analyzer::new(44_100, 2).unwrap();
        let tone: Vec<f32> = (0..44_100 * 4)
            .flat_map(|n| {
                let t = n as f64 / 44_100.0;
                let s = (0.3 * (std::f64::consts::TAU * 440.0 * t).sin()) as f32;
                [s, s]
            })
            .collect();
        let silence = vec![0.0; 44_100 * 2];
        analyzer.push(&silence).unwrap();
        analyzer.push(&tone).unwrap();
        let analysis = analyzer.finish(&AnalysisSettings::default()).unwrap();
        assert_near(analysis.cue_in_ms, 1_000, 6, "cue-in");
        assert_eq!(analysis.duration_ms, 5_000);
    }

    #[test]
    fn crossfade_point_respects_fade_min_when_the_last_frame_is_partial() {
        // 44.1 kHz frames are 5.011 ms long, so the track ends inside a frame.
        let mut analyzer = Analyzer::new(44_100, 2).unwrap();
        let tone: Vec<f32> = (0..44_100 * 3 + 37)
            .flat_map(|n| {
                let s =
                    (0.3 * (std::f64::consts::TAU * 440.0 * f64::from(n) / 44_100.0).sin()) as f32;
                [s, s]
            })
            .collect();
        analyzer.push(&tone).unwrap();
        let analysis = analyzer.finish(&AnalysisSettings::default()).unwrap();
        assert_eq!(analysis.cue_out_ms, analysis.duration_ms);
        assert_eq!(
            analysis.cue_out_ms - analysis.fade_out_start_ms.unwrap(),
            150
        );
    }

    #[test]
    fn garbage_levels_are_rejected() {
        let mut analyzer = Analyzer::new(RATE, 2).unwrap();
        analyzer.push(&vec![1_000.0; 48_000 * 2]).unwrap();
        assert!(matches!(
            analyzer.finish(&AnalysisSettings::default()),
            Err(Error::InvalidAudio { .. })
        ));
    }

    #[test]
    fn empty_input_is_an_error() {
        let analyzer = Analyzer::new(RATE, 2).unwrap();
        assert!(matches!(
            analyzer.finish(&AnalysisSettings::default()),
            Err(Error::Empty)
        ));
    }
}
