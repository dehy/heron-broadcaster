// SPDX-License-Identifier: AGPL-3.0-or-later

//! The mixer: Deck A, Deck B and the FX player summed on the master bus.
//!
//! This is the code of the audio thread. It never allocates, locks, does I/O
//! or logs: it talks to the rest of the engine through lock-free ring buffers
//! (commands in, events and finished tracks out) and atomics (status, levels).
//! Tracks are dropped by the control thread, never here.
//!
//! Transitions are decided here, sample-accurately, from a plan computed in
//! advance ([`FrameTransition`]). The same code renders to a buffer offline,
//! which is how crossfades are tested.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};

use heron_core::FadeCurve;
use heron_core::playout::{Ramp, TrackPlayout, Transition};

/// Internal sample rate.
pub const SAMPLE_RATE: u32 = 48_000;
/// Internal channel count (interleaved stereo).
pub const CHANNELS: usize = 2;
/// Frames rendered per block by the pacing clock (10 ms).
pub const BLOCK_FRAMES: usize = 480;

/// Fade applied when pausing, resuming or stopping the FX player.
const SHORT_FADE_FRAMES: u64 = 480;
/// Peak meters fall back by this much per second.
const PEAK_DECAY_DB_PER_S: f32 = 20.0;
/// Time constant of the RMS meters.
const RMS_TAU_S: f32 = 0.3;

pub fn ms_to_frames(ms: u32) -> u64 {
    u64::from(ms) * u64::from(SAMPLE_RATE) / 1_000
}

pub fn frames_to_ms(frames: u64) -> u64 {
    frames * 1_000 / u64::from(SAMPLE_RATE)
}

/// Decoded audio for one track, at the internal format.
pub trait AudioSource: Send {
    /// Reads up to `out.len() / 2` frames of interleaved stereo into `out`
    /// and returns the number of frames read. Must not block or allocate.
    fn read(&mut self, out: &mut [f32]) -> usize;
    /// No more audio will come: the track ended, early or not.
    fn is_finished(&self) -> bool;
}

/// Audio held in memory. Used for offline rendering and tests.
pub struct VecSource {
    samples: Vec<f32>,
    cursor: usize,
}

impl VecSource {
    pub fn new(samples: Vec<f32>) -> Self {
        Self { samples, cursor: 0 }
    }
}

impl AudioSource for VecSource {
    fn read(&mut self, out: &mut [f32]) -> usize {
        let len = out.len().min(self.samples.len() - self.cursor);
        let len = len - len % CHANNELS;
        out[..len].copy_from_slice(&self.samples[self.cursor..self.cursor + len]);
        self.cursor += len;
        len / CHANNELS
    }

    fn is_finished(&self) -> bool {
        self.cursor >= self.samples.len()
    }
}

/// A gain ramp in frames, on a track's timeline (frame 0 is the cue-in).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FrameRamp {
    pub start: u64,
    pub length: u64,
    pub curve: FadeCurve,
    /// Level at the quiet end of the ramp.
    pub level: f32,
}

impl FrameRamp {
    /// Converts a ramp planned in milliseconds on the file's timeline.
    pub fn from_ramp(ramp: &Ramp, cue_in_ms: u32) -> Self {
        Self {
            start: ms_to_frames(ramp.start_ms.saturating_sub(cue_in_ms)),
            length: ms_to_frames(ramp.length_ms).max(1),
            curve: ramp.curve,
            level: ramp.level,
        }
    }

    fn progress(&self, position: u64) -> f32 {
        (position.saturating_sub(self.start) as f64 / self.length as f64).min(1.0) as f32
    }

    pub fn fade_in_gain(&self, position: u64) -> f32 {
        if position < self.start {
            return self.level;
        }
        self.level + (1.0 - self.level) * self.curve.fade_in(self.progress(position))
    }

    pub fn fade_out_gain(&self, position: u64) -> f32 {
        if position < self.start {
            return 1.0;
        }
        self.level + (1.0 - self.level) * self.curve.fade_in(1.0 - self.progress(position))
    }
}

/// A transition from the track on air to the next one, in frames on the
/// outgoing track's timeline.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FrameTransition {
    /// When the next track starts.
    pub next_start: u64,
    pub fade_out: Option<FrameRamp>,
}

impl FrameTransition {
    pub fn new(transition: &Transition, outgoing: &TrackPlayout) -> Self {
        Self {
            next_start: ms_to_frames(transition.next_start_ms.saturating_sub(outgoing.cue_in_ms)),
            fade_out: transition
                .fade_out
                .map(|ramp| FrameRamp::from_ramp(&ramp, outgoing.cue_in_ms)),
        }
    }

    /// The next track starts as soon as the current one ends.
    pub const fn back_to_back() -> Self {
        Self {
            next_start: u64::MAX,
            fade_out: None,
        }
    }
}

/// A track ready to play.
pub struct LoadedTrack {
    /// Identifies the track in events and status (queue entry ID). Not 0.
    pub entry_id: u64,
    pub source: Box<dyn AudioSource>,
    /// Frames from the cue-in to the cue-out.
    pub length: u64,
    /// Linear gain (loudness normalization).
    pub gain: f32,
    pub fade_in: Option<FrameRamp>,
}

pub enum Command {
    /// Plays `track` next. It starts at `transition.next_start` on the track
    /// on air, when the track on air ends, or at once when nothing is on air.
    /// Replaces a track already cued.
    Cue {
        track: Box<LoadedTrack>,
        transition: FrameTransition,
    },
    /// Removes the cued track if it has not started.
    Uncue,
    /// Fades out the track on air and starts the cued track now.
    Skip {
        fade_frames: u64,
        curve: FadeCurve,
    },
    Pause,
    Resume,
    /// Plays a jingle on the FX player, on top of the decks. Replaces the one playing.
    PlayFx(Box<LoadedTrack>),
    StopFx,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Deck {
    A,
    B,
    Fx,
}

impl Deck {
    pub const ALL: [Deck; 3] = [Deck::A, Deck::B, Deck::Fx];

    fn from_slot(slot: usize) -> Self {
        match slot {
            0 => Self::A,
            1 => Self::B,
            _ => Self::Fx,
        }
    }

    pub fn slot(self) -> usize {
        match self {
            Self::A => 0,
            Self::B => 1,
            Self::Fx => 2,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EndReason {
    /// Reached its cue-out, or the end of the audio.
    Finished,
    Skipped,
    /// The FX player was stopped.
    Stopped,
    /// A new track needed the deck.
    Replaced,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    Started {
        deck: Deck,
        entry_id: u64,
        /// Master frame where the track starts.
        at_frame: u64,
    },
    Ended {
        deck: Deck,
        entry_id: u64,
        played_frames: u64,
        at_frame: u64,
        reason: EndReason,
    },
    /// The track's audio was not decoded in time; silence was played.
    Underrun { deck: Deck, entry_id: u64 },
    /// A cued track was removed before it started.
    Uncued { entry_id: u64 },
}

/// Levels of one meter point, as linear values (f32 bits).
#[derive(Debug, Default)]
pub struct MeterStatus {
    peak: [AtomicU32; CHANNELS],
    rms: [AtomicU32; CHANNELS],
}

impl MeterStatus {
    /// Peak (falling back 20 dB/s) and RMS (300 ms) per channel, linear.
    pub fn read(&self) -> ([f32; CHANNELS], [f32; CHANNELS]) {
        let load = |value: &AtomicU32| f32::from_bits(value.load(Ordering::Relaxed));
        (
            [load(&self.peak[0]), load(&self.peak[1])],
            [load(&self.rms[0]), load(&self.rms[1])],
        )
    }
}

#[derive(Debug, Default)]
pub struct DeckStatus {
    /// 0 when the deck is empty.
    pub entry_id: AtomicU64,
    pub position: AtomicU64,
    pub length: AtomicU64,
    pub meter: MeterStatus,
}

/// What the audio thread publishes after every block.
#[derive(Debug, Default)]
pub struct MixerStatus {
    /// Frames rendered since the start.
    pub frame: AtomicU64,
    pub paused: AtomicBool,
    /// Entry ID of the track on air (the last one started on Deck A or B), 0 for none.
    pub on_air: AtomicU64,
    /// Entry ID of the cued track, 0 for none.
    pub cued: AtomicU64,
    pub decks: [DeckStatus; 3],
    pub master: MeterStatus,
    pub underruns: AtomicU64,
    /// Events or finished tracks that could not be handed to the control
    /// thread because its queues were full.
    pub overflows: AtomicU64,
}

/// The control thread's side of the mixer.
pub struct MixerHandle {
    pub commands: rtrb::Producer<Command>,
    pub events: rtrb::Consumer<Event>,
    /// Finished tracks, to be dropped outside the audio thread.
    pub trash: rtrb::Consumer<Box<LoadedTrack>>,
    pub status: Arc<MixerStatus>,
}

struct Voice {
    track: Box<LoadedTrack>,
    position: u64,
    fade_out: Option<FrameRamp>,
    stop_at: u64,
    end_reason: EndReason,
    underrun_reported: bool,
}

impl Voice {
    fn new(track: Box<LoadedTrack>) -> Self {
        let stop_at = track.length;
        Self {
            track,
            position: 0,
            fade_out: None,
            stop_at,
            end_reason: EndReason::Finished,
            underrun_reported: false,
        }
    }

    /// Fades out from the current position and stops at the end of the fade.
    fn fade_out_now(&mut self, frames: u64, curve: FadeCurve, reason: EndReason) {
        let frames = frames.max(1);
        let current = self.gain_at(self.position);
        self.fade_out = Some(FrameRamp {
            start: self.position,
            length: frames,
            curve,
            level: 0.0,
        });
        // Keep the current fade-out level: scale the new fade from it.
        self.track.gain *= current;
        self.track.fade_in = None;
        self.stop_at = self.stop_at.min(self.position + frames);
        self.end_reason = reason;
    }

    fn gain_at(&self, position: u64) -> f32 {
        let fade_in = self
            .track
            .fade_in
            .map_or(1.0, |ramp| ramp.fade_in_gain(position));
        let fade_out = self
            .fade_out
            .map_or(1.0, |ramp| ramp.fade_out_gain(position));
        fade_in * fade_out
    }
}

#[derive(Clone, Copy, Default)]
struct BlockMeter {
    peak: [f32; CHANNELS],
    sum_squares: [f32; CHANNELS],
}

impl BlockMeter {
    fn add(&mut self, left: f32, right: f32) {
        self.peak[0] = self.peak[0].max(left.abs());
        self.peak[1] = self.peak[1].max(right.abs());
        self.sum_squares[0] += left * left;
        self.sum_squares[1] += right * right;
    }

    fn publish(&self, frames: usize, meter: &MeterStatus, decay: f32, alpha: f32) {
        for channel in 0..CHANNELS {
            let previous_peak = f32::from_bits(meter.peak[channel].load(Ordering::Relaxed));
            let peak = self.peak[channel].max(previous_peak * decay);
            meter.peak[channel].store(peak.to_bits(), Ordering::Relaxed);

            let previous = f32::from_bits(meter.rms[channel].load(Ordering::Relaxed));
            let mean_square = self.sum_squares[channel] / frames.max(1) as f32;
            let previous_square = previous * previous;
            let rms = (previous_square + alpha * (mean_square - previous_square)).sqrt();
            meter.rms[channel].store(rms.to_bits(), Ordering::Relaxed);
        }
    }
}

pub struct Mixer {
    /// Deck A, Deck B, FX.
    voices: [Option<Voice>; 3],
    /// Slot of the track on air (0 or 1).
    main: Option<usize>,
    cued: Option<(Box<LoadedTrack>, FrameTransition)>,
    frame: u64,
    pause_gain: f32,
    paused: bool,
    frozen: bool,
    scratch: Vec<f32>,
    meters: [BlockMeter; 3],
    commands: rtrb::Consumer<Command>,
    events: rtrb::Producer<Event>,
    trash: rtrb::Producer<Box<LoadedTrack>>,
    status: Arc<MixerStatus>,
}

impl Mixer {
    /// Creates a mixer rendering blocks of at most `max_block_frames`.
    pub fn new(max_block_frames: usize) -> (Self, MixerHandle) {
        let (command_tx, command_rx) = rtrb::RingBuffer::new(64);
        let (event_tx, event_rx) = rtrb::RingBuffer::new(256);
        let (trash_tx, trash_rx) = rtrb::RingBuffer::new(64);
        let status = Arc::new(MixerStatus::default());
        let mixer = Self {
            voices: [None, None, None],
            main: None,
            cued: None,
            frame: 0,
            pause_gain: 1.0,
            paused: false,
            frozen: false,
            scratch: vec![0.0; max_block_frames * CHANNELS],
            meters: [BlockMeter::default(); 3],
            commands: command_rx,
            events: event_tx,
            trash: trash_tx,
            status: Arc::clone(&status),
        };
        let handle = MixerHandle {
            commands: command_tx,
            events: event_rx,
            trash: trash_rx,
            status,
        };
        (mixer, handle)
    }

    /// Renders the next `out.len() / 2` frames of the master bus.
    pub fn render(&mut self, out: &mut [f32]) {
        let max = self.scratch.len();
        for block in out.chunks_mut(max) {
            self.render_block(block);
        }
    }

    fn render_block(&mut self, out: &mut [f32]) {
        let frames = out.len() / CHANNELS;
        let out = &mut out[..frames * CHANNELS];
        out.fill(0.0);
        self.apply_commands();
        self.meters = [BlockMeter::default(); 3];

        if !self.frozen {
            let mut offset = 0;
            while offset < frames {
                self.retire_finished();
                self.start_cued_if_due();
                let mut segment = self.next_segment(frames - offset);
                // The track on air first: when its audio ends early, the
                // segment ends there, so the next track starts on that frame.
                let main = self.main;
                if let Some(slot) = main {
                    let range = offset * CHANNELS..(offset + segment) * CHANNELS;
                    let read = self.render_voice(slot, &mut out[range]);
                    if self.voices[slot]
                        .as_ref()
                        .is_some_and(|voice| voice.position >= voice.stop_at)
                    {
                        segment = read;
                    }
                }
                for slot in (0..self.voices.len()).filter(|&slot| Some(slot) != main) {
                    let range = offset * CHANNELS..(offset + segment) * CHANNELS;
                    self.render_voice(slot, &mut out[range]);
                }
                offset += segment;
                self.frame += segment as u64;
            }
            self.retire_finished();
            self.start_cued_if_due();
            self.apply_pause(out);
        } else {
            self.frame += frames as u64;
        }

        let mut master = BlockMeter::default();
        for frame in out.as_chunks_mut::<CHANNELS>().0 {
            // No limiter before M9: never let anything over full scale out.
            frame[0] = frame[0].clamp(-1.0, 1.0);
            frame[1] = frame[1].clamp(-1.0, 1.0);
            master.add(frame[0], frame[1]);
        }
        self.publish(frames, &master);
    }

    fn apply_commands(&mut self) {
        while let Ok(command) = self.commands.pop() {
            match command {
                Command::Cue { track, transition } => {
                    if let Some((previous, _)) = self.cued.take() {
                        self.emit(Event::Uncued {
                            entry_id: previous.entry_id,
                        });
                        self.discard(previous);
                    }
                    self.cued = Some((track, transition));
                }
                Command::Uncue => {
                    if let Some((track, _)) = self.cued.take() {
                        self.emit(Event::Uncued {
                            entry_id: track.entry_id,
                        });
                        self.discard(track);
                    }
                }
                Command::Skip { fade_frames, curve } => self.skip(fade_frames, curve),
                Command::Pause => self.paused = true,
                Command::Resume => {
                    self.paused = false;
                    self.frozen = false;
                }
                Command::PlayFx(track) => {
                    self.stop_slot(2, EndReason::Replaced);
                    self.start_voice(2, track);
                }
                Command::StopFx => {
                    if let Some(voice) = &mut self.voices[2] {
                        voice.fade_out_now(
                            SHORT_FADE_FRAMES,
                            FadeCurve::Linear,
                            EndReason::Stopped,
                        );
                    }
                }
            }
        }
    }

    fn skip(&mut self, fade_frames: u64, curve: FadeCurve) {
        if let Some(main) = self.main
            && let Some(voice) = &mut self.voices[main]
        {
            voice.fade_out_now(fade_frames, curve, EndReason::Skipped);
        }
        if let Some((track, _)) = self.cued.take() {
            self.start_main(track, None);
        }
    }

    /// Starts the cued track when the track on air reaches the transition
    /// point or has ended.
    fn start_cued_if_due(&mut self) {
        let due = match (&self.cued, self.main_voice()) {
            (None, _) => false,
            (Some(_), None) => true,
            (Some((_, transition)), Some(voice)) => voice.position >= transition.next_start,
        };
        if due && let Some((track, transition)) = self.cued.take() {
            self.start_main(track, transition.fade_out);
        }
    }

    fn start_main(&mut self, track: Box<LoadedTrack>, fade_out: Option<FrameRamp>) {
        let slot = match self.main {
            Some(main) => {
                if let Some(voice) = &mut self.voices[main]
                    && voice.end_reason == EndReason::Finished
                {
                    voice.fade_out = fade_out;
                }
                1 - main
            }
            None => usize::from(self.voices[0].is_some() && self.voices[1].is_none()),
        };
        // Still busy with an older track's tail: it has to go.
        self.stop_slot(slot, EndReason::Replaced);
        self.start_voice(slot, track);
        self.main = Some(slot);
    }

    fn start_voice(&mut self, slot: usize, track: Box<LoadedTrack>) {
        self.emit(Event::Started {
            deck: Deck::from_slot(slot),
            entry_id: track.entry_id,
            at_frame: self.frame,
        });
        self.voices[slot] = Some(Voice::new(track));
    }

    fn main_voice(&self) -> Option<&Voice> {
        self.main.and_then(|slot| self.voices[slot].as_ref())
    }

    /// Frames until the next thing that must happen at an exact frame.
    /// Frames until the next thing that must happen at an exact frame: a
    /// track ending, or the cued track starting.
    fn next_segment(&self, remaining: usize) -> usize {
        let mut until = self
            .voices
            .iter()
            .flatten()
            .map(|voice| voice.stop_at.saturating_sub(voice.position))
            .min()
            .unwrap_or(u64::MAX);
        if let Some((_, transition)) = &self.cued
            && let Some(voice) = self.main_voice()
        {
            until = until.min(transition.next_start.saturating_sub(voice.position));
        }
        usize::try_from(until)
            .unwrap_or(usize::MAX)
            .clamp(1, remaining)
    }

    /// Adds the voice in `slot` to `out` and returns the frames it played.
    fn render_voice(&mut self, slot: usize, out: &mut [f32]) -> usize {
        let Some(voice) = &mut self.voices[slot] else {
            return 0;
        };
        let wanted = (out.len() / CHANNELS).min(
            usize::try_from(voice.stop_at.saturating_sub(voice.position)).unwrap_or(usize::MAX),
        );
        if wanted == 0 {
            return 0;
        }
        let scratch = &mut self.scratch[..wanted * CHANNELS];
        let read = voice.track.source.read(scratch);
        let meter = &mut self.meters[slot];
        for (index, (input, output)) in scratch[..read * CHANNELS]
            .as_chunks::<CHANNELS>().0.iter()
            .zip(out.as_chunks_mut::<CHANNELS>().0)
            .enumerate()
        {
            let gain = voice.track.gain * voice.gain_at(voice.position + index as u64);
            let left = input[0] * gain;
            let right = input[1] * gain;
            output[0] += left;
            output[1] += right;
            meter.add(left, right);
        }
        voice.position += read as u64;
        if read < wanted {
            if voice.track.source.is_finished() {
                // The audio ended before the cue-out.
                voice.stop_at = voice.position;
            } else {
                self.status.underruns.fetch_add(1, Ordering::Relaxed);
                if !voice.underrun_reported {
                    voice.underrun_reported = true;
                    let entry_id = voice.track.entry_id;
                    self.emit(Event::Underrun {
                        deck: Deck::from_slot(slot),
                        entry_id,
                    });
                }
            }
        }
        read
    }

    fn retire_finished(&mut self) {
        for slot in 0..self.voices.len() {
            // Sources that end early set `stop_at` when they are read.
            if let Some(voice) = &self.voices[slot]
                && voice.position >= voice.stop_at
            {
                let reason = voice.end_reason;
                self.stop_slot(slot, reason);
            }
        }
    }

    fn stop_slot(&mut self, slot: usize, reason: EndReason) {
        let Some(voice) = self.voices[slot].take() else {
            return;
        };
        if self.main == Some(slot) {
            self.main = None;
        }
        self.emit(Event::Ended {
            deck: Deck::from_slot(slot),
            entry_id: voice.track.entry_id,
            played_frames: voice.position,
            at_frame: self.frame,
            reason,
        });
        self.discard(voice.track);
    }

    fn apply_pause(&mut self, out: &mut [f32]) {
        let target = if self.paused { 0.0 } else { 1.0 };
        if self.pause_gain == target {
            if self.paused {
                out.fill(0.0);
                self.frozen = true;
            }
            return;
        }
        let step = 1.0 / SHORT_FADE_FRAMES as f32;
        for frame in out.as_chunks_mut::<CHANNELS>().0 {
            self.pause_gain = if self.paused {
                (self.pause_gain - step).max(0.0)
            } else {
                (self.pause_gain + step).min(1.0)
            };
            frame[0] *= self.pause_gain;
            frame[1] *= self.pause_gain;
        }
        if self.paused && self.pause_gain == 0.0 {
            self.frozen = true;
        }
    }

    fn emit(&mut self, event: Event) {
        if self.events.push(event).is_err() {
            self.status.overflows.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Hands a track to the control thread, which drops it.
    fn discard(&mut self, track: Box<LoadedTrack>) {
        if let Err(rtrb::PushError::Full(track)) = self.trash.push(track) {
            self.status.overflows.fetch_add(1, Ordering::Relaxed);
            // Never expected: the control thread empties the trash every few ms.
            // Leaking is better than freeing memory on the audio thread.
            std::mem::forget(track);
        }
    }

    fn publish(&self, frames: usize, master: &BlockMeter) {
        let seconds = frames as f32 / SAMPLE_RATE as f32;
        let decay = 10f32.powf(-PEAK_DECAY_DB_PER_S * seconds / 20.0);
        let alpha = 1.0 - (-seconds / RMS_TAU_S).exp();
        let status = &self.status;
        status.frame.store(self.frame, Ordering::Relaxed);
        status.paused.store(self.paused, Ordering::Relaxed);
        let main_id = self.main_voice().map_or(0, |voice| voice.track.entry_id);
        status.on_air.store(main_id, Ordering::Relaxed);
        let cued_id = self.cued.as_ref().map_or(0, |(track, _)| track.entry_id);
        status.cued.store(cued_id, Ordering::Relaxed);
        for (slot, deck) in status.decks.iter().enumerate() {
            let (entry_id, position, length) =
                self.voices[slot].as_ref().map_or((0, 0, 0), |voice| {
                    (voice.track.entry_id, voice.position, voice.track.length)
                });
            deck.entry_id.store(entry_id, Ordering::Relaxed);
            deck.position.store(position, Ordering::Relaxed);
            deck.length.store(length, Ordering::Relaxed);
            self.meters[slot].publish(frames, &deck.meter, decay, alpha);
        }
        master.publish(frames, &status.master, decay, alpha);
    }
}

#[cfg(test)]
mod tests;
