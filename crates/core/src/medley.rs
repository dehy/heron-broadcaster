// SPDX-License-Identifier: AGPL-3.0-or-later

//! Medleys: song hooks and liners picked from a recipe ([`plan`]), then
//! mixed into one stretch of audio ([`mix`]). The API plans, the librarian
//! decodes the parts and renders the file. See `docs/medleys.md`.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

/// Sample rate of the parts handed to [`mix`] and of the rendered file.
pub const SAMPLE_RATE: u32 = 48_000;
pub const CHANNELS: usize = 2;
/// Every part fades in and out over at least this, so no part clicks.
pub const EDGE_FADE_MS: u32 = 20;
/// The music goes down under a liner (and back up) over this.
pub const DUCK_RAMP_MS: u32 = 150;
/// The mix is scaled down when its peak would go over this.
pub const PEAK_LIMIT: f32 = 0.98;

/// Folder of the previews, in the buffer zone (hidden: never imported).
pub const PREVIEW_FOLDER: &str = ".medley-previews";

/// Where a medley's preview goes, relative to the buffer zone.
pub fn preview_path(medley_id: u64) -> String {
    format!("{PREVIEW_FOLDER}/medley-{medley_id}.wav")
}

/// How a medley is built.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(default, deny_unknown_fields)]
pub struct Recipe {
    /// Categories the songs come from.
    pub category_ids: Vec<u64>,
    /// Only the songs with this label, when set.
    pub label_id: Option<u64>,
    /// How many hooks.
    pub hooks: u32,
    pub order: HookOrder,
    /// Hooks shorter than this are left out.
    pub hook_min_ms: u32,
    /// Hooks longer than this are cut to it.
    pub hook_max_ms: u32,
    /// Overlap between two parts.
    pub crossfade_ms: u32,
    /// Where the liners come from; none for no liners.
    pub liner_category_id: Option<u64>,
    /// A liner after every this many hooks.
    pub liner_every: u32,
    pub liner_at_start: bool,
    pub liner_at_end: bool,
    pub liner_mode: LinerMode,
    /// How much the music goes down under a liner played over it.
    pub duck_db: f64,
}

impl Default for Recipe {
    fn default() -> Self {
        Self {
            category_ids: Vec::new(),
            label_id: None,
            hooks: 6,
            order: HookOrder::Picked,
            hook_min_ms: 4_000,
            hook_max_ms: 8_000,
            crossfade_ms: 500,
            liner_category_id: None,
            liner_every: 1,
            liner_at_start: false,
            liner_at_end: false,
            liner_mode: LinerMode::Between,
            duck_db: -10.0,
        }
    }
}

impl Recipe {
    /// Why the recipe cannot be built, if it cannot.
    pub fn check(&self) -> Result<(), String> {
        if self.category_ids.is_empty() {
            return Err("pick at least one category for the songs".to_owned());
        }
        if !(2..=30).contains(&self.hooks) {
            return Err("a medley has 2 to 30 hooks".to_owned());
        }
        if self.hook_min_ms < 1_000
            || self.hook_min_ms > self.hook_max_ms
            || self.hook_max_ms > 60_000
        {
            return Err("hooks last 1 to 60 s, the minimum at most the maximum".to_owned());
        }
        if self.crossfade_ms > 5_000 || self.crossfade_ms >= self.hook_min_ms {
            return Err("the crossfade is at most 5 s, and shorter than the hooks".to_owned());
        }
        if self.liner_every == 0 {
            return Err("liners come every 1 hook or more".to_owned());
        }
        if !(-40.0..=0.0).contains(&self.duck_db) {
            return Err("ducking is between -40 and 0 dB".to_owned());
        }
        Ok(())
    }
}

/// The order of the hooks.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum HookOrder {
    /// As picked (at random).
    #[default]
    Picked,
    /// Oldest first.
    Year,
    /// Slowest first.
    Bpm,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum LinerMode {
    /// Liners take their own time between the hooks.
    #[default]
    Between,
    /// Liners start on the transition, the music lowered under them.
    Over,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum PartKind {
    Hook,
    Liner,
}

/// A span of a track's file in the medley.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Part {
    pub kind: PartKind,
    pub track_id: u64,
    /// On the file's timeline.
    pub from_ms: u32,
    pub to_ms: u32,
}

impl Part {
    pub fn length_ms(&self) -> u32 {
        self.to_ms.saturating_sub(self.from_ms)
    }
}

/// A track that can be part of a medley: a song with its hook, or a liner
/// with its cue points.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Candidate {
    pub track_id: u64,
    pub from_ms: u32,
    pub to_ms: u32,
    pub year: Option<u16>,
    pub bpm: Option<f64>,
}

/// A small random generator (xorshift): picks need no cryptographic randomness.
pub struct Picker(u64);

impl Picker {
    pub fn new(seed: u64) -> Self {
        Self(seed | 1)
    }

    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    pub fn shuffle<T>(&mut self, items: &mut [T]) {
        for i in (1..items.len()).rev() {
            let j = (self.next() % (i as u64 + 1)) as usize;
            items.swap(i, j);
        }
    }
}

/// The songs whose hook is long enough, shuffled, those not in `avoid`
/// (the medley's current version) first.
fn usable_songs(
    recipe: &Recipe,
    songs: &[Candidate],
    avoid: &HashSet<u64>,
    picker: &mut Picker,
) -> Vec<Candidate> {
    let mut usable: Vec<Candidate> = songs
        .iter()
        .filter(|song| song.to_ms.saturating_sub(song.from_ms) >= recipe.hook_min_ms)
        .copied()
        .collect();
    picker.shuffle(&mut usable);
    // Stable: the shuffle holds within each group.
    usable.sort_by_key(|song| avoid.contains(&song.track_id));
    usable
}

fn hook(recipe: &Recipe, song: &Candidate) -> Part {
    Part {
        kind: PartKind::Hook,
        track_id: song.track_id,
        from_ms: song.from_ms,
        to_ms: song
            .to_ms
            .min(song.from_ms.saturating_add(recipe.hook_max_ms)),
    }
}

fn liner(candidate: &Candidate) -> Part {
    Part {
        kind: PartKind::Liner,
        track_id: candidate.track_id,
        from_ms: candidate.from_ms,
        to_ms: candidate.to_ms,
    }
}

/// How many songs can give a hook.
pub fn qualified(recipe: &Recipe, songs: &[Candidate]) -> usize {
    songs
        .iter()
        .filter(|song| song.to_ms.saturating_sub(song.from_ms) >= recipe.hook_min_ms)
        .count()
}

/// Picks the parts of a medley: `recipe.hooks` hooks (fewer when not
/// enough songs qualify), never the same song twice, songs not in `avoid`
/// first; liners where the recipe puts them, never the same twice in a row.
pub fn plan(
    recipe: &Recipe,
    songs: &[Candidate],
    liners: &[Candidate],
    avoid: &HashSet<u64>,
    seed: u64,
) -> Vec<Part> {
    let mut picker = Picker::new(seed);
    let mut picked: Vec<Candidate> = usable_songs(recipe, songs, avoid, &mut picker)
        .into_iter()
        .take(recipe.hooks as usize)
        .collect();
    match recipe.order {
        HookOrder::Picked => {}
        // Unknown values last.
        HookOrder::Year => picked.sort_by_key(|song| song.year.unwrap_or(u16::MAX)),
        HookOrder::Bpm => picked.sort_by(|a, b| {
            a.bpm
                .unwrap_or(f64::MAX)
                .total_cmp(&b.bpm.unwrap_or(f64::MAX))
        }),
    }

    let mut liners: Vec<Candidate> = liners
        .iter()
        .filter(|l| l.to_ms > l.from_ms)
        .copied()
        .collect();
    picker.shuffle(&mut liners);
    let mut next_liner = 0;
    let mut previous: Option<u64> = None;
    let mut take_liner = |parts: &mut Vec<Part>| {
        if liners.is_empty() {
            return;
        }
        let mut candidate = liners[next_liner % liners.len()];
        next_liner += 1;
        if previous == Some(candidate.track_id) && liners.len() > 1 {
            candidate = liners[next_liner % liners.len()];
            next_liner += 1;
        }
        previous = Some(candidate.track_id);
        parts.push(liner(&candidate));
    };

    let mut parts = Vec::new();
    if recipe.liner_at_start && !picked.is_empty() {
        take_liner(&mut parts);
    }
    let count = picked.len();
    for (index, song) in picked.iter().enumerate() {
        parts.push(hook(recipe, song));
        let done = index + 1;
        if done < count && done % recipe.liner_every as usize == 0 {
            take_liner(&mut parts);
        }
    }
    if recipe.liner_at_end && count > 0 {
        take_liner(&mut parts);
    }
    parts
}

/// Every part of `kind` the candidates can give, in their order: hooks cut
/// to the recipe's maximum (those too short left out), liners whole.
pub fn choices(recipe: &Recipe, kind: PartKind, candidates: &[Candidate]) -> Vec<Part> {
    candidates
        .iter()
        .filter(|candidate| match kind {
            PartKind::Hook => {
                candidate.to_ms.saturating_sub(candidate.from_ms) >= recipe.hook_min_ms
            }
            PartKind::Liner => candidate.to_ms > candidate.from_ms,
        })
        .map(|candidate| match kind {
            PartKind::Hook => hook(recipe, candidate),
            PartKind::Liner => liner(candidate),
        })
        .collect()
}

/// Another part of `kind`, none of `exclude`: to replace one in a plan.
pub fn pick_one(
    recipe: &Recipe,
    kind: PartKind,
    candidates: &[Candidate],
    exclude: &HashSet<u64>,
    avoid: &HashSet<u64>,
    seed: u64,
) -> Option<Part> {
    let mut picker = Picker::new(seed);
    match kind {
        PartKind::Hook => usable_songs(recipe, candidates, avoid, &mut picker)
            .into_iter()
            .find(|song| !exclude.contains(&song.track_id))
            .map(|song| hook(recipe, &song)),
        PartKind::Liner => {
            let mut liners: Vec<Candidate> = candidates
                .iter()
                .filter(|l| l.to_ms > l.from_ms && !exclude.contains(&l.track_id))
                .copied()
                .collect();
            picker.shuffle(&mut liners);
            liners.first().map(liner)
        }
    }
}

/// The audio of a part: interleaved stereo at [`SAMPLE_RATE`], and the gain
/// bringing it to the station's loudness target.
pub struct PartAudio {
    pub kind: PartKind,
    pub samples: Vec<f32>,
    pub gain: f32,
}

pub fn ms_to_frames(ms: u32) -> usize {
    (u64::from(ms) * u64::from(SAMPLE_RATE) / 1_000) as usize
}

/// Adds `part` to `out` from frame `at`, fading in over `fade_in` frames
/// and out over `fade_out`, times `gain`.
fn add(out: &mut Vec<f32>, at: usize, part: &PartAudio, fade_in: usize, fade_out: usize) {
    let frames = part.samples.len() / CHANNELS;
    let end = at + frames;
    if out.len() < end * CHANNELS {
        out.resize(end * CHANNELS, 0.0);
    }
    let fade_in = fade_in.min(frames / 2).max(1);
    let fade_out = fade_out.min(frames / 2).max(1);
    for frame in 0..frames {
        let mut gain = part.gain;
        if frame < fade_in {
            gain *= frame as f32 / fade_in as f32;
        }
        if frame + fade_out > frames {
            gain *= (frames - frame) as f32 / fade_out as f32;
        }
        for channel in 0..CHANNELS {
            out[(at + frame) * CHANNELS + channel] +=
                part.samples[frame * CHANNELS + channel] * gain;
        }
    }
}

/// Mixes the parts, in order, into one interleaved stereo stretch:
/// consecutive parts overlap by the crossfade; with liners over the music,
/// each liner starts where the next hook does (at the start for a first
/// liner, so that it ends with the last hook for a last one) and the music
/// is lowered under it. The result is scaled down if it would clip.
pub fn mix(recipe: &Recipe, parts: &[PartAudio]) -> Vec<f32> {
    let crossfade = ms_to_frames(recipe.crossfade_ms);
    let edge = ms_to_frames(EDGE_FADE_MS);
    let fade = crossfade.max(edge);
    let over = recipe.liner_mode == LinerMode::Over;

    let mut music = Vec::new();
    // Where each liner over the music goes: (start frame, part).
    let mut placed: Vec<(usize, &PartAudio)> = Vec::new();
    // Liners waiting for the start of the next hook.
    let mut waiting: Vec<&PartAudio> = Vec::new();
    let mut cursor = 0usize;
    let mut first = true;
    let in_sequence = |part: &&PartAudio| !(over && part.kind == PartKind::Liner);
    let last = parts.iter().filter(in_sequence).count();
    let mut index = 0;
    for part in parts {
        if !in_sequence(&part) {
            waiting.push(part);
            continue;
        }
        index += 1;
        let at = if first {
            0
        } else {
            cursor.saturating_sub(crossfade)
        };
        // The medley's own start and end only fade over the edge.
        let fade_in = if first { edge } else { fade };
        let fade_out = if index == last { edge } else { fade };
        first = false;
        for liner in waiting.drain(..) {
            placed.push((at, liner));
        }
        add(&mut music, at, part, fade_in, fade_out);
        cursor = at + part.samples.len() / CHANNELS;
    }
    // Liners after the last hook end with it.
    for liner in waiting.drain(..) {
        let frames = liner.samples.len() / CHANNELS;
        placed.push((cursor.saturating_sub(frames), liner));
    }

    if !placed.is_empty() {
        let total = music.len() / CHANNELS;
        let mut envelope = vec![1.0f32; total];
        let duck = 10f32.powf(recipe.duck_db as f32 / 20.0);
        let ramp = ms_to_frames(DUCK_RAMP_MS).max(1);
        for (start, liner) in &placed {
            let end = (start + liner.samples.len() / CHANNELS).min(total);
            for (frame, level) in envelope.iter_mut().enumerate() {
                // Down over `ramp` before the liner, back up over `ramp` after it.
                let gain = if frame + ramp <= *start || frame >= end + ramp {
                    1.0
                } else if frame < *start {
                    let t = (start - frame) as f32 / ramp as f32;
                    duck + (1.0 - duck) * t
                } else if frame >= end {
                    let t = (frame - end) as f32 / ramp as f32;
                    duck + (1.0 - duck) * t
                } else {
                    duck
                };
                *level = level.min(gain);
            }
        }
        for (frame, level) in envelope.iter().enumerate() {
            music[frame * CHANNELS] *= level;
            music[frame * CHANNELS + 1] *= level;
        }
        for (start, liner) in placed {
            add(&mut music, start, liner, edge, edge);
        }
    }

    let peak = music
        .iter()
        .fold(0.0f32, |peak, sample| peak.max(sample.abs()));
    if peak > PEAK_LIMIT {
        let scale = PEAK_LIMIT / peak;
        for sample in &mut music {
            *sample *= scale;
        }
    }
    music
}

#[cfg(test)]
mod tests {
    use super::*;

    fn song(track_id: u64, from_ms: u32, to_ms: u32) -> Candidate {
        Candidate {
            track_id,
            from_ms,
            to_ms,
            year: None,
            bpm: None,
        }
    }

    fn recipe() -> Recipe {
        Recipe {
            category_ids: vec![1],
            ..Recipe::default()
        }
    }

    #[test]
    fn recipes_are_checked() {
        assert!(recipe().check().is_ok());
        assert!(Recipe::default().check().is_err(), "no category");
        for wrong in [
            Recipe {
                hooks: 1,
                ..recipe()
            },
            Recipe {
                hook_min_ms: 9_000,
                ..recipe()
            },
            Recipe {
                crossfade_ms: 4_000,
                ..recipe()
            },
            Recipe {
                liner_every: 0,
                ..recipe()
            },
            Recipe {
                duck_db: 3.0,
                ..recipe()
            },
        ] {
            assert!(wrong.check().is_err(), "{wrong:?}");
        }
    }

    #[test]
    fn hooks_are_cut_never_repeated_and_new_songs_come_first() {
        let songs: Vec<Candidate> = (1..=10)
            .map(|id| song(id, 1_000, if id == 10 { 3_000 } else { 21_000 }))
            .collect();
        let avoid: HashSet<u64> = [1, 2, 3, 4].into();
        let recipe = Recipe {
            hooks: 5,
            ..recipe()
        };
        let parts = plan(&recipe, &songs, &[], &avoid, 42);
        assert_eq!(parts.len(), 5);
        let ids: HashSet<u64> = parts.iter().map(|part| part.track_id).collect();
        assert_eq!(ids.len(), 5, "never the same song twice");
        // 5, 6, 7, 8, 9 qualify and were not in the last version; 10's hook is too short.
        assert_eq!(ids, [5, 6, 7, 8, 9].into());
        assert!(
            parts
                .iter()
                .all(|part| part.length_ms() == 8_000 && part.from_ms == 1_000)
        );
        assert_eq!(qualified(&recipe, &songs), 9);
    }

    #[test]
    fn hooks_follow_the_order_asked() {
        let songs = [
            Candidate {
                year: Some(1999),
                ..song(1, 0, 9_000)
            },
            Candidate {
                year: None,
                ..song(2, 0, 9_000)
            },
            Candidate {
                year: Some(1984),
                ..song(3, 0, 9_000)
            },
        ];
        let recipe = Recipe {
            hooks: 3,
            order: HookOrder::Year,
            ..recipe()
        };
        let ids: Vec<u64> = plan(&recipe, &songs, &[], &HashSet::new(), 7)
            .iter()
            .map(|part| part.track_id)
            .collect();
        assert_eq!(ids, [3, 1, 2]);
    }

    #[test]
    fn liners_go_where_the_recipe_says_never_twice_in_a_row() {
        let songs: Vec<Candidate> = (1..=6).map(|id| song(id, 0, 10_000)).collect();
        let liners = [song(100, 0, 2_000), song(101, 0, 2_000)];
        let recipe = Recipe {
            hooks: 6,
            liner_category_id: Some(9),
            liner_every: 2,
            liner_at_start: true,
            liner_at_end: true,
            ..recipe()
        };
        for seed in 1..20 {
            let parts = plan(&recipe, &songs, &liners, &HashSet::new(), seed);
            let kinds: String = parts
                .iter()
                .map(|part| {
                    if part.kind == PartKind::Hook {
                        'H'
                    } else {
                        'L'
                    }
                })
                .collect();
            assert_eq!(kinds, "LHHLHHLHHL");
            let liners: Vec<u64> = parts
                .iter()
                .filter(|part| part.kind == PartKind::Liner)
                .map(|part| part.track_id)
                .collect();
            assert!(
                liners.windows(2).all(|pair| pair[0] != pair[1]),
                "{liners:?}"
            );
        }
    }

    #[test]
    fn choices_are_cut_like_picks() {
        let songs = [song(1, 0, 20_000), song(2, 0, 3_000)];
        let parts = choices(&recipe(), PartKind::Hook, &songs);
        assert_eq!(parts.len(), 1, "the short hook is left out");
        assert_eq!((parts[0].track_id, parts[0].to_ms), (1, 8_000));
        let liners = choices(&recipe(), PartKind::Liner, &[song(3, 100, 2_000)]);
        assert_eq!((liners[0].kind, liners[0].to_ms), (PartKind::Liner, 2_000));
    }

    #[test]
    fn a_replacement_is_none_of_the_excluded() {
        let songs: Vec<Candidate> = (1..=3).map(|id| song(id, 0, 10_000)).collect();
        let exclude: HashSet<u64> = [1, 2].into();
        let part = pick_one(
            &recipe(),
            PartKind::Hook,
            &songs,
            &exclude,
            &HashSet::new(),
            3,
        );
        assert_eq!(part.map(|part| part.track_id), Some(3));
        let all: HashSet<u64> = [1, 2, 3].into();
        assert_eq!(
            pick_one(&recipe(), PartKind::Hook, &songs, &all, &HashSet::new(), 3),
            None
        );
    }

    fn constant(kind: PartKind, frames: usize, value: f32) -> PartAudio {
        PartAudio {
            kind,
            samples: vec![value; frames * CHANNELS],
            gain: 1.0,
        }
    }

    #[test]
    fn parts_overlap_by_the_crossfade() {
        let recipe = Recipe {
            crossfade_ms: 500,
            ..recipe()
        };
        let parts = [
            constant(PartKind::Hook, 48_000, 0.25),
            constant(PartKind::Liner, 48_000, 0.25),
            constant(PartKind::Hook, 48_000, 0.25),
        ];
        let out = mix(&recipe, &parts);
        // Three parts of 1 s, minus two crossfades of 0.5 s.
        assert_eq!(out.len() / CHANNELS, 3 * 48_000 - 2 * 24_000);
        // In the middle of the first part: its level; at the very start: faded in.
        assert!((out[12_000 * CHANNELS] - 0.25).abs() < 1e-6);
        assert!(out[0].abs() < 1e-6);
        // Linear crossfades add up to the same level.
        let middle = 48_000 - 24_000 + 12_000;
        assert!(
            (out[middle * CHANNELS] - 0.25).abs() < 1e-3,
            "{}",
            out[middle * CHANNELS]
        );
    }

    #[test]
    fn liners_over_the_music_duck_it() {
        let recipe = Recipe {
            crossfade_ms: 0,
            liner_mode: LinerMode::Over,
            duck_db: -20.0,
            ..recipe()
        };
        let parts = [
            constant(PartKind::Hook, 96_000, 0.5),
            constant(PartKind::Liner, 24_000, 0.0),
            constant(PartKind::Hook, 96_000, 0.5),
        ];
        let out = mix(&recipe, &parts);
        // The liner takes no time of its own.
        assert_eq!(out.len() / CHANNELS, 192_000);
        // It starts with the second hook: the music is at -20 dB under it,
        // and back to its level well before and after.
        let under = 96_000 + 12_000;
        assert!(
            (out[under * CHANNELS] - 0.05).abs() < 1e-4,
            "{}",
            out[under * CHANNELS]
        );
        assert!((out[48_000 * CHANNELS] - 0.5).abs() < 1e-6);
        assert!((out[170_000 * CHANNELS] - 0.5).abs() < 1e-6);
    }

    #[test]
    fn loud_mixes_are_scaled_down() {
        let parts = [PartAudio {
            gain: 2.0,
            ..constant(PartKind::Hook, 48_000, 0.9)
        }];
        let out = mix(&recipe(), &parts);
        let peak = out
            .iter()
            .fold(0.0f32, |peak, sample| peak.max(sample.abs()));
        assert!((peak - PEAK_LIMIT).abs() < 1e-6, "{peak}");
    }
}
