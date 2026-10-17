// SPDX-License-Identifier: AGPL-3.0-or-later

//! The running engine: the audio thread, the player, the outputs, and the
//! task keeping them in step.

use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use heron_core::{OutputsConfig, PlayoutSettings};
use heron_outputs::{ConfigError, NowPlaying, Outputs};
use heron_service::HealthCheck;
use tokio::sync::{broadcast, watch};

use crate::clock::{AudioThread, ClockStatus};
use crate::config::{ConfigFileError, EngineConfig};
use crate::mixer::{BLOCK_FRAMES, CHANNELS, Mixer, MixerStatus, SAMPLE_RATE};
use crate::player::{Player, PlayerEvent};

/// Audio waiting between the audio thread and the encoders, at most.
const MASTER_BUFFER_SECONDS: usize = 2;
/// How often the player handles the mixer's events.
const TICK: Duration = Duration::from_millis(10);

pub struct EngineOptions {
    pub library_dir: PathBuf,
    pub state_dir: PathBuf,
}

pub struct Engine {
    player: Mutex<Player>,
    outputs: Mutex<Outputs>,
    state_dir: PathBuf,
    events: broadcast::Sender<PlayerEvent>,
    status: Arc<MixerStatus>,
    clock: Arc<ClockStatus>,
    started: Instant,
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
        let player = Player::new(
            options.library_dir,
            config.playout,
            handle,
            events.clone(),
            now_playing_tx,
        );
        let clock = Arc::new(ClockStatus::default());
        let audio = AudioThread::spawn(mixer, master_tx, Arc::clone(&clock))?;
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
            started: Instant::now(),
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
    }
}

/// Ticks the player until the engine is dropped.
async fn run_player(engine: std::sync::Weak<Engine>) {
    let mut interval = tokio::time::interval(TICK);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut ticks = 0_u64;
    loop {
        interval.tick().await;
        let Some(engine) = engine.upgrade() else {
            return;
        };
        engine.player().tick();
        ticks += 1;
        if ticks.is_multiple_of(100) {
            engine.record_metrics();
        }
    }
}

/// `/healthz`: healthy while the audio thread renders.
#[derive(Clone)]
pub struct EngineHealth(pub Arc<MixerStatus>);

impl HealthCheck for EngineHealth {
    async fn check(&self) -> Result<(), String> {
        let before = self.0.frame.load(Ordering::Relaxed);
        tokio::time::sleep(Duration::from_millis(100)).await;
        if self.0.frame.load(Ordering::Relaxed) > before {
            Ok(())
        } else {
            Err("the audio thread is stuck".to_owned())
        }
    }
}
