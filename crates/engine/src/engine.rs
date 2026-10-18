// SPDX-License-Identifier: AGPL-3.0-or-later

//! The running engine: the audio thread, the player, the outputs, and the
//! task keeping them in step and saving the queue.

use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant, SystemTime};

use heron_core::{OutputsConfig, PlayoutSettings};
use heron_outputs::{ConfigError, NowPlaying, Outputs};
use heron_service::HealthCheck;
use tokio::sync::{broadcast, watch};

use crate::clock::{AudioThread, ClockStatus};
use crate::config::{ConfigFileError, EngineConfig};
use crate::fallback::Fallback;
use crate::mixer::{BLOCK_FRAMES, CHANNELS, Mixer, MixerStatus, SAMPLE_RATE};
use crate::player::{Player, PlayerEvent};
use crate::queue_file::{self, QueueWriter};

/// Audio waiting between the audio thread and the encoders, at most.
const MASTER_BUFFER_SECONDS: usize = 2;
/// How often the player handles the mixer's events.
const TICK: Duration = Duration::from_millis(10);
/// The queue is saved when it changes, and this often for the position.
const SAVE_EVERY: Duration = Duration::from_secs(5);
/// With the audio thread stuck this long, the process exits so Docker
/// restarts it, resuming the saved queue.
const STUCK_EXIT: Duration = Duration::from_secs(10);

pub struct EngineOptions {
    pub library_dir: PathBuf,
    pub state_dir: PathBuf,
    /// Tracks played when the queue runs dry. `None`: silence.
    pub fallback_dir: Option<PathBuf>,
    /// Queue entries whose file is read into memory ahead of time.
    pub preload_tracks: usize,
    /// Exit the process when the audio thread is stuck (production).
    pub exit_when_stuck: bool,
    /// Play the master bus on the local sound card (`monitor` feature).
    pub monitor: bool,
}

impl EngineOptions {
    /// Options for an engine without fallback directory.
    pub fn new(library_dir: PathBuf, state_dir: PathBuf) -> Self {
        Self {
            library_dir,
            state_dir,
            fallback_dir: None,
            preload_tracks: crate::player::PRELOAD_TRACKS,
            exit_when_stuck: false,
            monitor: false,
        }
    }
}

pub struct Engine {
    player: Mutex<Player>,
    outputs: Mutex<Outputs>,
    state_dir: PathBuf,
    events: broadcast::Sender<PlayerEvent>,
    status: Arc<MixerStatus>,
    clock: Arc<ClockStatus>,
    queue_writer: QueueWriter,
    started: Instant,
    started_at: SystemTime,
    #[cfg(feature = "monitor")]
    _monitor: Option<crate::monitor::Monitor>,
    _audio: AudioThread,
}

#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    #[error(transparent)]
    Config(#[from] ConfigError),
    #[error(transparent)]
    ConfigFile(#[from] ConfigFileError),
    #[error("cannot start a thread: {0}")]
    Thread(#[from] std::io::Error),
}

impl Engine {
    /// Starts playing (silence until tracks are queued) and streaming to the
    /// saved outputs.
    pub fn start(
        options: EngineOptions,
        runtime: tokio::runtime::Handle,
    ) -> Result<Arc<Self>, EngineError> {
        // Present from the start, so dashboards and alerts see them before
        // the first event.
        for counter in [
            "heron_engine_silence_skips_total",
            "heron_engine_fallback_tracks_total",
            "heron_engine_preload_failures_total",
            "heron_engine_underrun_tracks_total",
        ] {
            metrics::counter!(counter).absolute(0);
        }
        metrics::gauge!("heron_engine_dead_air").set(0.0);
        let config = match EngineConfig::load(&options.state_dir) {
            Ok(config) => config,
            Err(error) => {
                // Better on air with defaults than off air.
                tracing::error!(%error, "ignoring the saved configuration");
                EngineConfig::default()
            }
        };

        let (mixer, handle) = Mixer::new(BLOCK_FRAMES);
        let status = Arc::clone(&handle.status);
        let (master_tx, master_rx) =
            rtrb::RingBuffer::new(MASTER_BUFFER_SECONDS * SAMPLE_RATE as usize * CHANNELS);
        let (now_playing_tx, now_playing_rx) = watch::channel(NowPlaying::default());
        let (events, _) = broadcast::channel(256);

        let mut outputs = Outputs::new(master_rx, now_playing_rx, runtime.clone())?;
        if let Err(error) = outputs.apply(&config.outputs) {
            tracing::error!(%error, "cannot start the saved outputs");
        }
        #[cfg(feature = "monitor")]
        let monitor = if options.monitor {
            crate::monitor::Monitor::start(outputs.tap())
                .inspect_err(|error| tracing::error!(%error, "cannot start the monitor output"))
                .ok()
        } else {
            None
        };
        #[cfg(not(feature = "monitor"))]
        if options.monitor {
            tracing::warn!("built without the monitor feature: no monitor output");
        }
        let mut player = Player::new(
            options.library_dir,
            config.playout,
            handle,
            events.clone(),
            now_playing_tx,
        )
        .with_preload(options.preload_tracks);
        if let Some(dir) = options.fallback_dir {
            player = player.with_fallback(Fallback::start(dir, &options.state_dir));
        }
        player.restore(queue_file::load(&options.state_dir));
        let queue_writer = QueueWriter::start(options.state_dir.clone())?;
        let clock = Arc::new(ClockStatus::default());
        let audio = AudioThread::spawn(mixer, master_tx, Arc::clone(&clock))?;
        if options.exit_when_stuck {
            exit_when_stuck(Arc::clone(&status))?;
        }
        tracing::info!(
            outputs = config.outputs.outputs.len(),
            "audio engine started"
        );

        let engine = Arc::new(Self {
            player: Mutex::new(player),
            outputs: Mutex::new(outputs),
            state_dir: options.state_dir,
            events,
            status,
            clock,
            queue_writer,
            started: Instant::now(),
            started_at: SystemTime::now(),
            #[cfg(feature = "monitor")]
            _monitor: monitor,
            _audio: audio,
        });
        runtime.spawn(run_player(Arc::downgrade(&engine)));
        Ok(engine)
    }

    pub fn player(&self) -> MutexGuard<'_, Player> {
        self.player
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    pub fn outputs(&self) -> MutexGuard<'_, Outputs> {
        self.outputs
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// When the engine started.
    pub fn started_at(&self) -> SystemTime {
        self.started_at
    }

    pub fn subscribe(&self) -> broadcast::Receiver<PlayerEvent> {
        self.events.subscribe()
    }

    pub fn status(&self) -> &MixerStatus {
        &self.status
    }

    pub fn status_handle(&self) -> Arc<MixerStatus> {
        Arc::clone(&self.status)
    }

    pub fn config(&self) -> EngineConfig {
        EngineConfig {
            playout: *self.player().settings(),
            outputs: self.outputs().config().clone(),
        }
    }

    pub fn set_playout_settings(&self, settings: PlayoutSettings) -> Result<(), EngineError> {
        self.player().set_settings(settings);
        self.save()
    }

    /// Applies new outputs; an invalid configuration changes nothing.
    pub fn set_outputs(&self, config: &OutputsConfig) -> Result<(), EngineError> {
        self.outputs().apply(config)?;
        self.save()
    }

    fn save(&self) -> Result<(), EngineError> {
        self.config().save(&self.state_dir)?;
        Ok(())
    }

    /// Saves the queue now, on the calling thread (when shutting down).
    pub fn save_queue(&self) {
        let queue = self.player().saved();
        match queue_file::save(&self.state_dir, &queue) {
            Ok(()) => tracing::info!(waiting = queue.waiting.len(), "queue saved"),
            Err(error) => tracing::error!(%error, "cannot save the queue"),
        }
    }

    /// Problems listed in `/healthz` without making the engine unhealthy.
    pub fn warnings(&self) -> Vec<String> {
        let mut warnings = Vec::new();
        if let Some(warning) = heron_service::clock::status().warning {
            warnings.push(warning);
        }
        if self.player().dead_air() {
            warnings.push(
                "nothing to play: the queue is empty and the fallback directory has no track"
                    .to_owned(),
            );
        }
        warnings
    }

    fn record_metrics(&self) {
        let status = &self.status;
        let rendered = status.frame.load(Ordering::Relaxed) as f64 / f64::from(SAMPLE_RATE);
        metrics::gauge!("heron_engine_rendered_seconds").set(rendered);
        // Audio rendered minus time elapsed: constant (the render lead) when
        // the clock keeps time; it moves only when the clock resets.
        metrics::gauge!("heron_engine_clock_drift_seconds")
            .set(rendered - self.started.elapsed().as_secs_f64());
        let late_us = self.clock.max_late_us.swap(0, Ordering::Relaxed);
        metrics::gauge!("heron_engine_clock_max_late_seconds").set(late_us as f64 / 1e6);
        metrics::counter!("heron_engine_clock_resets_total")
            .absolute(self.clock.resets.load(Ordering::Relaxed));
        metrics::counter!("heron_engine_dropped_blocks_total")
            .absolute(self.clock.dropped_blocks.load(Ordering::Relaxed));
        metrics::counter!("heron_engine_underruns_total")
            .absolute(status.underruns.load(Ordering::Relaxed));
        metrics::counter!("heron_engine_mixer_overflows_total")
            .absolute(status.overflows.load(Ordering::Relaxed));
        metrics::gauge!("heron_engine_paused")
            .set(f64::from(u8::from(status.paused.load(Ordering::Relaxed))));
        let (peak, _) = status.master.read();
        metrics::gauge!("heron_engine_master_peak").set(f64::from(peak[0].max(peak[1])));
        metrics::gauge!("heron_engine_queue_length")
            .set(self.player().snapshot().queue.len() as f64);
        metrics::gauge!("heron_engine_silent_seconds")
            .set(status.silent_frames.load(Ordering::Relaxed) as f64 / f64::from(SAMPLE_RATE));
        heron_service::clock::status().record_metrics();
    }
}

/// Exits the process when the audio thread stops rendering: Docker's
/// restart policy brings the engine back, with the saved queue.
fn exit_when_stuck(status: Arc<MixerStatus>) -> std::io::Result<()> {
    std::thread::Builder::new()
        .name("heron-watchdog".to_owned())
        .spawn(move || {
            let mut last = status.frame.load(Ordering::Relaxed);
            let mut since = Instant::now();
            loop {
                std::thread::sleep(Duration::from_secs(1));
                let frame = status.frame.load(Ordering::Relaxed);
                if frame != last {
                    last = frame;
                    since = Instant::now();
                } else if since.elapsed() >= STUCK_EXIT {
                    tracing::error!(
                        seconds = STUCK_EXIT.as_secs(),
                        "the audio thread is stuck: exiting to be restarted"
                    );
                    std::process::exit(1);
                }
            }
        })?;
    Ok(())
}

/// Ticks the player until the engine is dropped.
async fn run_player(engine: std::sync::Weak<Engine>) {
    let mut interval = tokio::time::interval(TICK);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut ticks = 0_u64;
    let mut saved_at = Instant::now();
    loop {
        interval.tick().await;
        let Some(engine) = engine.upgrade() else {
            return;
        };
        let saved = {
            let mut player = engine.player();
            player.tick();
            (player.take_changed() || saved_at.elapsed() >= SAVE_EVERY).then(|| player.saved())
        };
        if let Some(queue) = saved {
            engine.queue_writer.submit(queue);
            saved_at = Instant::now();
        }
        ticks += 1;
        if ticks.is_multiple_of(100) {
            engine.record_metrics();
        }
    }
}

/// `/healthz`: healthy while the audio thread renders. Warns when the clock
/// is not synchronized or there is nothing to play.
#[derive(Clone)]
pub struct EngineHealth(pub Arc<Engine>);

impl HealthCheck for EngineHealth {
    async fn check(&self) -> Result<(), String> {
        let status = self.0.status();
        let before = status.frame.load(Ordering::Relaxed);
        tokio::time::sleep(Duration::from_millis(100)).await;
        if status.frame.load(Ordering::Relaxed) > before {
            Ok(())
        } else {
            Err("the audio thread is stuck".to_owned())
        }
    }

    async fn warnings(&self) -> Vec<String> {
        self.0.warnings()
    }
}
