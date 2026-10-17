// SPDX-License-Identifier: AGPL-3.0-or-later

//! The player: the control side of the mixer.
//!
//! It owns the queue (in memory: the engine never reads the database),
//! loads the next track onto a free deck as soon as the current one starts,
//! plans the transition between them, and turns the mixer's events into
//! track changes for the outputs and the API. It runs outside the audio
//! thread and may allocate, lock and log.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::time::SystemTime;

use heron_core::playout::{TrackPlayout, Transition, db_to_gain, plan_transition};
use heron_core::{CrossfadeSettings, PlaySource, PlayoutSettings};
use heron_outputs::NowPlaying;
use tokio::sync::{broadcast, watch};

use crate::decoder::DecodedSource;
use crate::mixer::{
    AudioSource, Command, Deck, EndReason, Event, FrameRamp, FrameTransition, LoadedTrack,
    MixerHandle, MixerStatus, frames_to_ms, ms_to_frames,
};

/// Opens the audio of a file between its cue points (in ms).
pub type SourceLoader = fn(PathBuf, u32, u32) -> std::io::Result<Box<dyn AudioSource>>;

/// Decodes on a thread of its own, ahead of the deck.
fn spawn_decoder(
    path: PathBuf,
    cue_in_ms: u32,
    cue_out_ms: u32,
) -> std::io::Result<Box<dyn AudioSource>> {
    Ok(Box::new(DecodedSource::spawn(path, cue_in_ms, cue_out_ms)?))
}

/// A track to play, as sent by the API.
#[derive(Clone, Debug, PartialEq)]
pub struct TrackRequest {
    pub track_id: Option<u64>,
    /// Relative to the library root, with `/` separators.
    pub path: String,
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub playout: TrackPlayout,
    /// Carried back in the events, for the play history.
    pub source: PlaySource,
}

#[derive(Clone, Debug, PartialEq)]
pub struct QueueEntry {
    pub entry_id: u64,
    pub track: TrackRequest,
}

#[derive(Clone, Debug, PartialEq)]
pub enum PlayerEvent {
    Started {
        deck: Deck,
        entry: QueueEntry,
        at: SystemTime,
    },
    Ended {
        deck: Deck,
        entry: QueueEntry,
        played_ms: u64,
        reason: EndReason,
        at: SystemTime,
    },
}

#[derive(Debug, thiserror::Error)]
pub enum PlayerError {
    #[error("invalid path {0:?}: it must be relative to the library, without `..`")]
    InvalidPath(String),
    #[error("no queue entry {0}")]
    NotFound(u64),
    #[error("entry {0} is on air: skip it instead")]
    OnAir(u64),
    #[error("the engine is busy, try again")]
    Busy,
    #[error("cannot start decoding: {0}")]
    Spawn(#[from] std::io::Error),
}

/// One deck as seen from outside.
#[derive(Clone, Debug, PartialEq)]
pub struct DeckSnapshot {
    pub deck: Deck,
    pub entry: Option<QueueEntry>,
    pub position_ms: u64,
    pub length_ms: u64,
    pub on_air: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PlayerSnapshot {
    pub paused: bool,
    pub decks: Vec<DeckSnapshot>,
    pub cued: Option<QueueEntry>,
    pub queue: Vec<QueueEntry>,
    pub rendered_ms: u64,
    pub underruns: u64,
}

pub struct Player {
    library_dir: PathBuf,
    settings: PlayoutSettings,
    mixer: MixerHandle,
    queue: VecDeque<QueueEntry>,
    /// Entries handed to the mixer (cued, on air, fading out, FX).
    loaded: HashMap<u64, QueueEntry>,
    cued: Option<u64>,
    /// Entries the player uncued whose `Uncued` event has not arrived yet.
    /// They are not cued again before it does: the mixer may have started
    /// them instead.
    uncueing: HashSet<u64>,
    /// The track on air: the last one started on Deck A or B.
    on_air: Option<u64>,
    paused: bool,
    next_entry_id: u64,
    load_source: SourceLoader,
    events: broadcast::Sender<PlayerEvent>,
    now_playing: watch::Sender<NowPlaying>,
}

impl Player {
    pub fn new(
        library_dir: PathBuf,
        settings: PlayoutSettings,
        mixer: MixerHandle,
        events: broadcast::Sender<PlayerEvent>,
        now_playing: watch::Sender<NowPlaying>,
    ) -> Self {
        Self {
            library_dir,
            settings,
            mixer,
            queue: VecDeque::new(),
            loaded: HashMap::new(),
            cued: None,
            uncueing: HashSet::new(),
            on_air: None,
            paused: false,
            next_entry_id: 1,
            load_source: spawn_decoder,
            events,
            now_playing,
        }
    }

    /// Replaces how tracks are opened (decode threads by default).
    pub fn with_source_loader(mut self, load_source: SourceLoader) -> Self {
        self.load_source = load_source;
        self
    }

    pub fn status(&self) -> &MixerStatus {
        &self.mixer.status
    }

    /// Adds tracks to the queue at `position` (0 is next), or at the end.
    pub fn enqueue(
        &mut self,
        tracks: Vec<TrackRequest>,
        position: Option<usize>,
    ) -> Result<Vec<u64>, PlayerError> {
        for track in &tracks {
            self.resolve(&track.path)?;
        }
        let entries: Vec<QueueEntry> = tracks
            .into_iter()
            .map(|track| {
                let entry_id = self.next_entry_id;
                self.next_entry_id += 1;
                QueueEntry { entry_id, track }
            })
            .collect();
        let ids = entries.iter().map(|entry| entry.entry_id).collect();
        let position = position.unwrap_or(usize::MAX);
        if position == 0 {
            // The new tracks go before the one already cued.
            self.uncue()?;
        }
        let at = position.min(self.queue.len());
        for (offset, entry) in entries.into_iter().enumerate() {
            self.queue.insert(at + offset, entry);
        }
        self.ensure_cued();
        Ok(ids)
    }

    pub fn remove(&mut self, entry_id: u64) -> Result<(), PlayerError> {
        if self.on_air == Some(entry_id) {
            return Err(PlayerError::OnAir(entry_id));
        }
        if self.cued == Some(entry_id) {
            self.uncue()?;
        }
        let before = self.queue.len();
        self.queue.retain(|entry| entry.entry_id != entry_id);
        if self.queue.len() == before {
            return Err(PlayerError::NotFound(entry_id));
        }
        self.ensure_cued();
        Ok(())
    }

    /// Moves a waiting entry (cued or queued) to `position` (0 is next).
    pub fn move_entry(&mut self, entry_id: u64, position: usize) -> Result<(), PlayerError> {
        if self.on_air == Some(entry_id) || self.is_playing(entry_id) {
            return Err(PlayerError::OnAir(entry_id));
        }
        if self.cued == Some(entry_id) && position == 0 {
            return Ok(());
        }
        if self.cued.is_some() && (self.cued == Some(entry_id) || position == 0) {
            // The cued track goes back to the front of the queue.
            self.uncue()?;
        }
        let Some(from) = self.queue.iter().position(|e| e.entry_id == entry_id) else {
            self.ensure_cued();
            return Err(PlayerError::NotFound(entry_id));
        };
        if let Some(entry) = self.queue.remove(from) {
            let at = position.min(self.queue.len());
            self.queue.insert(at, entry);
        }
        self.ensure_cued();
        Ok(())
    }

    /// Replaces the track of a waiting entry (cued or queued), keeping its
    /// place and ID. A cued entry is loaded again.
    pub fn replace(&mut self, entry_id: u64, track: TrackRequest) -> Result<(), PlayerError> {
        self.resolve(&track.path)?;
        if self.on_air == Some(entry_id) || self.is_playing(entry_id) {
            return Err(PlayerError::OnAir(entry_id));
        }
        if self.cued == Some(entry_id) {
            self.uncue()?;
        }
        let Some(entry) = self.queue.iter_mut().find(|e| e.entry_id == entry_id) else {
            return Err(PlayerError::NotFound(entry_id));
        };
        entry.track = track;
        self.ensure_cued();
        Ok(())
    }

    /// Whether the entry is on a deck (on air, fading out or on the FX player).
    fn is_playing(&self, entry_id: u64) -> bool {
        self.cued != Some(entry_id)
            && self.loaded.contains_key(&entry_id)
            && !self.queue.iter().any(|e| e.entry_id == entry_id)
    }

    /// Empties the queue, including the cued track. The track on air plays on.
    pub fn clear(&mut self) -> Result<(), PlayerError> {
        self.uncue()?;
        self.queue.clear();
        Ok(())
    }

    /// Fades out the track on air and starts the next one.
    pub fn skip(&mut self) -> Result<(), PlayerError> {
        let crossfade = &self.settings.crossfade;
        self.send(Command::Skip {
            fade_frames: ms_to_frames(crossfade.skip_fade_ms),
            curve: crossfade.fade_out.curve,
        })
    }

    pub fn pause(&mut self) -> Result<(), PlayerError> {
        self.send(Command::Pause)?;
        self.paused = true;
        Ok(())
    }

    pub fn resume(&mut self) -> Result<(), PlayerError> {
        self.send(Command::Resume)?;
        self.paused = false;
        Ok(())
    }

    pub fn play_fx(&mut self, track: TrackRequest) -> Result<u64, PlayerError> {
        let entry = QueueEntry {
            entry_id: self.next_entry_id,
            track,
        };
        self.next_entry_id += 1;
        let loaded = self.load(&entry, None)?;
        self.send(Command::PlayFx(loaded))?;
        let entry_id = entry.entry_id;
        self.loaded.insert(entry_id, entry);
        Ok(entry_id)
    }

    pub fn stop_fx(&mut self) -> Result<(), PlayerError> {
        self.send(Command::StopFx)
    }

    pub fn settings(&self) -> &PlayoutSettings {
        &self.settings
    }

    /// New settings apply to the next transitions planned.
    pub fn set_settings(&mut self, settings: PlayoutSettings) {
        self.settings = settings;
    }

    /// Handles what the mixer reported and keeps the next track cued. Call
    /// it every few milliseconds.
    pub fn tick(&mut self) {
        while let Ok(track) = self.mixer.trash.pop() {
            // Stops its decode thread.
            drop(track);
        }
        while let Ok(event) = self.mixer.events.pop() {
            self.handle(event);
        }
        self.ensure_cued();
    }

    fn handle(&mut self, event: Event) {
        match event {
            Event::Started { deck, entry_id, .. } => {
                // The mixer may start a track the player just tried to uncue.
                self.uncueing.remove(&entry_id);
                self.queue.retain(|entry| entry.entry_id != entry_id);
                if self.cued == Some(entry_id) {
                    self.cued = None;
                }
                let Some(entry) = self.loaded.get(&entry_id).cloned() else {
                    return;
                };
                if deck != Deck::Fx {
                    self.on_air = Some(entry_id);
                    self.now_playing.send_replace(NowPlaying {
                        artist: entry.track.artist.clone(),
                        title: entry.track.title.clone(),
                    });
                    self.retarget_cued();
                }
                tracing::info!(
                    ?deck,
                    entry_id,
                    artist = entry.track.artist.as_deref().unwrap_or_default(),
                    title = entry.track.title.as_deref().unwrap_or_default(),
                    "track started"
                );
                let _ = self.events.send(PlayerEvent::Started {
                    deck,
                    entry,
                    at: SystemTime::now(),
                });
            }
            Event::Ended {
                deck,
                entry_id,
                played_frames,
                reason,
                ..
            } => {
                if self.on_air == Some(entry_id) {
                    self.on_air = None;
                }
                if let Some(entry) = self.loaded.remove(&entry_id) {
                    let _ = self.events.send(PlayerEvent::Ended {
                        deck,
                        entry,
                        played_ms: frames_to_ms(played_frames),
                        reason,
                        at: SystemTime::now(),
                    });
                }
            }
            Event::Uncued { entry_id } => {
                if self.uncueing.remove(&entry_id) {
                    // Uncued by the player: back in the queue.
                    if self.cued != Some(entry_id) {
                        self.loaded.remove(&entry_id);
                    }
                    return;
                }
                // Dropped by the mixer, its audio having ended before it started.
                if self.cued == Some(entry_id) {
                    let path = self.loaded.get(&entry_id).map(|e| e.track.path.clone());
                    tracing::warn!(entry_id, ?path, "skipping a track that cannot be decoded");
                    self.cued = None;
                }
                self.loaded.remove(&entry_id);
            }
            Event::Underrun { deck, entry_id } => {
                metrics::counter!("heron_engine_underrun_tracks_total").increment(1);
                tracing::warn!(?deck, entry_id, "decoding fell behind: silence played");
            }
        }
    }

    /// Loads the next queue entry onto the mixer when nothing is cued.
    fn ensure_cued(&mut self) {
        while self.cued.is_none() {
            if self
                .queue
                .front()
                .is_none_or(|entry| self.uncueing.contains(&entry.entry_id))
            {
                return;
            }
            let Some(entry) = self.queue.pop_front() else {
                return;
            };
            let transition = self.transition_into(&entry.track.playout);
            match self.load(&entry, transition.as_ref()) {
                Ok(track) => {
                    let frame_transition = self.frame_transition(transition.as_ref());
                    match self.mixer.commands.push(Command::Cue {
                        track,
                        transition: frame_transition,
                    }) {
                        Ok(()) => {
                            self.cued = Some(entry.entry_id);
                            self.loaded.insert(entry.entry_id, entry);
                        }
                        Err(_) => {
                            // Try again on the next tick.
                            self.queue.push_front(entry);
                            return;
                        }
                    }
                }
                Err(error) => {
                    tracing::error!(entry_id = entry.entry_id, path = %entry.track.path, %error, "cannot load track");
                }
            }
        }
    }

    /// Removes the cued track from the mixer and puts it back in front of the queue.
    fn uncue(&mut self) -> Result<(), PlayerError> {
        let Some(cued) = self.cued.take() else {
            return Ok(());
        };
        if let Err(error) = self.send(Command::Uncue) {
            self.cued = Some(cued);
            return Err(error);
        }
        self.uncueing.insert(cued);
        if let Some(entry) = self.loaded.get(&cued).cloned() {
            self.queue.push_front(entry);
        }
        Ok(())
    }

    /// Replans the cued track's transition against the track now on air.
    fn retarget_cued(&mut self) {
        let Some(cued) = self.cued else {
            return;
        };
        let Some(entry) = self.loaded.get(&cued) else {
            return;
        };
        let transition = self.transition_into(&entry.track.playout);
        let fade_in = self.fade_in(entry, transition.as_ref());
        let command = Command::Retarget {
            transition: self.frame_transition(transition.as_ref()),
            fade_in,
        };
        let _ = self.send(command);
    }

    /// The transition from the track on air into `incoming`; `None` when
    /// nothing is on air.
    fn transition_into(&self, incoming: &TrackPlayout) -> Option<Transition> {
        let outgoing = self.loaded.get(&self.on_air?)?;
        Some(plan_transition(
            &outgoing.track.playout,
            incoming,
            &self.crossfade(),
        ))
    }

    fn frame_transition(&self, transition: Option<&Transition>) -> FrameTransition {
        let outgoing = self.on_air.and_then(|entry_id| self.loaded.get(&entry_id));
        match (transition, outgoing) {
            (Some(transition), Some(outgoing)) => {
                FrameTransition::new(transition, &outgoing.track.playout)
            }
            _ => FrameTransition::back_to_back(),
        }
    }

    fn fade_in(&self, entry: &QueueEntry, transition: Option<&Transition>) -> Option<FrameRamp> {
        transition
            .and_then(|transition| transition.fade_in)
            .map(|ramp| FrameRamp::from_ramp(&ramp, entry.track.playout.cue_in_ms))
    }

    fn crossfade(&self) -> CrossfadeSettings {
        self.settings.crossfade
    }

    fn load(
        &self,
        entry: &QueueEntry,
        transition: Option<&Transition>,
    ) -> Result<Box<LoadedTrack>, PlayerError> {
        let playout = &entry.track.playout;
        let path = self.resolve(&entry.track.path)?;
        let source = (self.load_source)(path, playout.cue_in_ms, playout.cue_out_ms)?;
        Ok(Box::new(LoadedTrack {
            entry_id: entry.entry_id,
            source,
            length: ms_to_frames(playout.playable_ms()),
            gain: db_to_gain(playout.gain_db(&self.settings.loudness)),
            fade_in: self.fade_in(entry, transition),
        }))
    }

    fn send(&mut self, command: Command) -> Result<(), PlayerError> {
        self.mixer
            .commands
            .push(command)
            .map_err(|_| PlayerError::Busy)
    }

    fn resolve(&self, path: &str) -> Result<PathBuf, PlayerError> {
        resolve_library_path(&self.library_dir, path)
            .ok_or_else(|| PlayerError::InvalidPath(path.to_owned()))
    }

    pub fn snapshot(&self) -> PlayerSnapshot {
        let status = &self.mixer.status;
        let on_air = status.on_air.load(Ordering::Relaxed);
        let decks = Deck::ALL
            .iter()
            .map(|&deck| {
                let deck_status = &status.decks[deck.slot()];
                let entry_id = deck_status.entry_id.load(Ordering::Relaxed);
                DeckSnapshot {
                    deck,
                    entry: self.loaded.get(&entry_id).cloned(),
                    position_ms: frames_to_ms(deck_status.position.load(Ordering::Relaxed)),
                    length_ms: frames_to_ms(deck_status.length.load(Ordering::Relaxed)),
                    on_air: entry_id != 0 && entry_id == on_air,
                }
            })
            .collect();
        PlayerSnapshot {
            paused: self.paused,
            decks,
            cued: self.cued.and_then(|cued| self.loaded.get(&cued).cloned()),
            queue: self.queue.iter().cloned().collect(),
            rendered_ms: frames_to_ms(status.frame.load(Ordering::Relaxed)),
            underruns: status.underruns.load(Ordering::Relaxed),
        }
    }
}

/// Resolves a library path, refusing anything that could leave the library.
pub fn resolve_library_path(root: &Path, path: &str) -> Option<PathBuf> {
    let mut resolved = root.to_path_buf();
    for part in path.split('/') {
        if part.is_empty() || part == "." || part == ".." || part.contains('\\') {
            return None;
        }
        resolved.push(part);
    }
    Some(resolved)
}

#[cfg(test)]
mod tests;
