// SPDX-License-Identifier: AGPL-3.0-or-later

//! From the master bus to every output.
//!
//! ```text
//! audio thread ──ring──► fan-out thread ──┬─► encoder thread (MP3 128k) ──┬─► output task (Icecast A)
//!                                         │                               └─► output task (Icecast B)
//!                                         └─► encoder thread (MP3 64k)  ────► output task (Icecast C)
//! ```
//!
//! Every hop is bounded and drops instead of blocking: a slow encoder loses
//! audio without slowing the others, and a slow output loses audio without
//! slowing its encoder or the other outputs.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, SyncSender, TrySendError};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, SystemTime};

use bytes::Bytes;
use heron_core::{EncoderProfile, OutputConfig, OutputKind, OutputsConfig};
use tokio::sync::{broadcast, mpsc, oneshot, watch};

use crate::ConfigError;
use crate::connection::HandedConnection;
use crate::encoder::{self, Encoder, INPUT_CHANNELS};
use crate::icecast::{IcecastConfig, IcecastSink};
use crate::sink::{self, NowPlaying, OutputState, OutputStatus, ReleaseRequest};

/// Frames handed to the encoders at once, at most.
const CHUNK_FRAMES: usize = 4_800;
/// Audio chunks waiting for an encoder, at most (several seconds).
const ENCODER_QUEUE: usize = 256;
/// Encoded chunks waiting for an output, at most.
const OUTPUT_QUEUE: usize = 512;
const POLL: Duration = Duration::from_millis(5);

type Targets = Arc<Mutex<Vec<(u64, SyncSender<Arc<[f32]>>)>>>;

/// The thread reading the master bus and handing it to every encoder.
struct Fanout {
    targets: Targets,
    /// Master bus frames after which nothing is handed on (`u64::MAX`: none).
    limit: Arc<AtomicU64>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Fanout {
    fn spawn(mut master: rtrb::Consumer<f32>) -> std::io::Result<Self> {
        let targets: Targets = Arc::default();
        let limit = Arc::new(AtomicU64::new(u64::MAX));
        let stop = Arc::new(AtomicBool::new(false));
        let thread_targets = Arc::clone(&targets);
        let thread_limit = Arc::clone(&limit);
        let thread_stop = Arc::clone(&stop);
        let thread = std::thread::Builder::new()
            .name("heron-fanout".to_owned())
            .spawn(move || {
                // Frames read from the master bus since the start.
                let mut read = 0_u64;
                while !thread_stop.load(Ordering::Relaxed) {
                    let available = master.slots().min(CHUNK_FRAMES * INPUT_CHANNELS);
                    let available = available - available % INPUT_CHANNELS;
                    let Ok(chunk) = master.read_chunk(available) else {
                        continue;
                    };
                    if available == 0 {
                        chunk.commit_all();
                        std::thread::sleep(POLL);
                        continue;
                    }
                    let (first, second) = chunk.as_slices();
                    let mut audio = [first, second].concat();
                    chunk.commit_all();
                    let frames = (audio.len() / INPUT_CHANNELS) as u64;
                    let wanted = thread_limit.load(Ordering::Relaxed).saturating_sub(read);
                    read += frames;
                    if wanted < frames {
                        audio.truncate(usize::try_from(wanted).unwrap_or(0) * INPUT_CHANNELS);
                        if audio.is_empty() {
                            continue;
                        }
                    }
                    let audio: Arc<[f32]> = audio.into();
                    let Ok(targets) = thread_targets.lock() else {
                        break;
                    };
                    for (profile, sender) in targets.iter() {
                        if let Err(TrySendError::Full(_)) = sender.try_send(Arc::clone(&audio)) {
                            metrics::counter!(
                                "heron_encoder_dropped_chunks_total",
                                "profile" => profile.to_string()
                            )
                            .increment(1);
                        }
                    }
                }
            })?;
        Ok(Self {
            targets,
            limit,
            stop,
            thread: Some(thread),
        })
    }

    fn add(&self, profile: u64, sender: SyncSender<Arc<[f32]>>) {
        if let Ok(mut targets) = self.targets.lock() {
            targets.push((profile, sender));
        }
    }

    fn remove(&self, profile: u64) {
        if let Ok(mut targets) = self.targets.lock() {
            targets.retain(|(id, _)| *id != profile);
        }
    }
}

impl Drop for Fanout {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// One encoder profile in use: its thread and the stream it publishes.
struct EncoderPipeline {
    profile: EncoderProfile,
    info: encoder::StreamInfo,
    audio: broadcast::Sender<Bytes>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl EncoderPipeline {
    fn spawn(profile: &EncoderProfile, fanout: &Fanout) -> Result<Self, ConfigError> {
        let mut encoder = encoder::build(profile)?;
        let info = encoder.info().clone();
        let (audio, _) = broadcast::channel(OUTPUT_QUEUE);
        let (sender, receiver) = std::sync::mpsc::sync_channel(ENCODER_QUEUE);
        let stop = Arc::new(AtomicBool::new(false));
        let thread_audio = audio.clone();
        let thread_stop = Arc::clone(&stop);
        let name = profile.name.clone();
        let thread = std::thread::Builder::new()
            .name("heron-encoder".to_owned())
            .spawn(move || {
                encode(
                    &name,
                    encoder.as_mut(),
                    &receiver,
                    &thread_audio,
                    &thread_stop,
                )
            })
            .map_err(|error| ConfigError::Profile {
                profile: profile.name.clone(),
                reason: format!("cannot start the encoder thread: {error}"),
            })?;
        fanout.add(profile.id, sender);
        Ok(Self {
            profile: profile.clone(),
            info,
            audio,
            stop,
            thread: Some(thread),
        })
    }

    fn stop(mut self, fanout: &Fanout) {
        self.finish(fanout);
    }

    /// Stops encoding once what the encoder was given is encoded and
    /// flushed. Outputs keep receiving what was published.
    fn finish(&mut self, fanout: &Fanout) {
        fanout.remove(self.profile.id);
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// Encodes until `stop`, then what is left: the audio already handed to
/// the encoder, and what the encoder holds.
fn encode(
    name: &str,
    encoder: &mut dyn Encoder,
    receiver: &Receiver<Arc<[f32]>>,
    audio: &broadcast::Sender<Bytes>,
    stop: &AtomicBool,
) {
    let mut encoded = Vec::new();
    let mut failing = false;
    let mut publish = |result: Result<(), encoder::EncodeError>, encoded: &[u8]| {
        match result {
            Ok(()) => failing = false,
            Err(error) => {
                if !failing {
                    tracing::error!(profile = name, %error, "encoding failed");
                }
                failing = true;
                return;
            }
        }
        if !encoded.is_empty() {
            // No subscriber (every output disconnected) is fine.
            let _ = audio.send(Bytes::copy_from_slice(encoded));
        }
    };
    while !stop.load(Ordering::Relaxed) {
        let pcm = match receiver.recv_timeout(Duration::from_millis(100)) {
            Ok(pcm) => pcm,
            Err(RecvTimeoutError::Timeout) => continue,
            Err(RecvTimeoutError::Disconnected) => break,
        };
        encoded.clear();
        let result = encoder.encode(&pcm, &mut encoded);
        publish(result, &encoded);
    }
    while let Ok(pcm) = receiver.try_recv() {
        encoded.clear();
        let result = encoder.encode(&pcm, &mut encoded);
        publish(result, &encoded);
    }
    encoded.clear();
    let result = encoder.flush(&mut encoded);
    publish(result, &encoded);
}

struct RunningOutput {
    config: OutputConfig,
    status: Arc<OutputStatus>,
    task: tokio::task::JoinHandle<()>,
    release: mpsc::Sender<ReleaseRequest>,
}

/// A snapshot of one output, for the API and the dashboard.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutputSnapshot {
    pub id: u64,
    pub name: String,
    pub enabled: bool,
    pub state: Option<OutputState>,
    pub bytes_sent: u64,
    pub dropped_chunks: u64,
}

/// Every encoder and output of the station.
pub struct Outputs {
    runtime: tokio::runtime::Handle,
    fanout: Fanout,
    metadata: watch::Receiver<NowPlaying>,
    config: OutputsConfig,
    encoders: HashMap<u64, EncoderPipeline>,
    running: HashMap<u64, RunningOutput>,
    /// The relay network outputs connect through (`heron-engine relay`).
    relay: Option<PathBuf>,
}

impl Outputs {
    /// Starts reading the master bus. Nothing is encoded or sent until
    /// [`apply`](Self::apply) configures outputs.
    pub fn new(
        master: rtrb::Consumer<f32>,
        metadata: watch::Receiver<NowPlaying>,
        runtime: tokio::runtime::Handle,
    ) -> std::io::Result<Self> {
        Ok(Self {
            runtime,
            fanout: Fanout::spawn(master)?,
            metadata,
            config: OutputsConfig::default(),
            encoders: HashMap::new(),
            running: HashMap::new(),
            relay: None,
        })
    }

    /// Network outputs connect through the relay listening on `relay`
    /// from now on (when they next connect).
    pub fn set_relay(&mut self, relay: Option<PathBuf>) {
        self.relay = relay;
    }

    /// Checks a configuration without applying it.
    pub fn validate(config: &OutputsConfig) -> Result<(), ConfigError> {
        let mut names = std::collections::HashSet::new();
        for profile in &config.encoder_profiles {
            encoder::build(profile)?;
        }
        for output in &config.outputs {
            if !names.insert(output.name.as_str()) {
                return Err(ConfigError::Output {
                    output: output.name.clone(),
                    reason: "two outputs have this name".to_owned(),
                });
            }
            if !config
                .encoder_profiles
                .iter()
                .any(|profile| profile.id == output.encoder_profile_id)
            {
                return Err(ConfigError::Output {
                    output: output.name.clone(),
                    reason: format!("unknown encoder profile {}", output.encoder_profile_id),
                });
            }
            sink_config(output)?;
        }
        Ok(())
    }

    /// Applies a new configuration. Outputs whose settings and encoder are
    /// unchanged keep streaming without a hiccup.
    pub fn apply(&mut self, config: &OutputsConfig) -> Result<(), ConfigError> {
        self.apply_adopting(config, HashMap::new())
    }

    /// Applies a new configuration, going on with the connections in
    /// `adopted` (by output ID) instead of connecting those outputs: an
    /// engine taking over from another ([`Self::release`]). A connection
    /// left unused is closed.
    pub fn apply_adopting(
        &mut self,
        config: &OutputsConfig,
        mut adopted: HashMap<u64, HandedConnection>,
    ) -> Result<(), ConfigError> {
        Self::validate(config)?;
        let profiles: HashMap<u64, &EncoderProfile> = config
            .encoder_profiles
            .iter()
            .map(|profile| (profile.id, profile))
            .collect();
        let enabled: Vec<&OutputConfig> = config
            .outputs
            .iter()
            .filter(|output| output.enabled)
            .collect();

        // Encoders run only while an enabled output uses them.
        let wanted: HashMap<u64, &EncoderProfile> = enabled
            .iter()
            .filter_map(|output| profiles.get(&output.encoder_profile_id))
            .map(|profile| (profile.id, *profile))
            .collect();
        let stale: Vec<u64> = self
            .encoders
            .iter()
            .filter(|(id, pipeline)| wanted.get(id) != Some(&&pipeline.profile))
            .map(|(id, _)| *id)
            .collect();
        for id in &stale {
            if let Some(pipeline) = self.encoders.remove(id) {
                pipeline.stop(&self.fanout);
            }
        }

        // Stop outputs that changed, were removed, or lost their encoder.
        let keep: Vec<u64> = self
            .running
            .iter()
            .filter(|(id, output)| {
                enabled
                    .iter()
                    .any(|new| new.id == **id && **new == output.config)
                    && !stale.contains(&output.config.encoder_profile_id)
            })
            .map(|(id, _)| *id)
            .collect();
        self.running.retain(|id, output| {
            let kept = keep.contains(id);
            if !kept {
                output.task.abort();
            }
            kept
        });

        for (id, profile) in &wanted {
            if !self.encoders.contains_key(id) {
                let pipeline = EncoderPipeline::spawn(profile, &self.fanout)?;
                tracing::info!(profile = %profile.name, "encoder started");
                self.encoders.insert(*id, pipeline);
            }
        }
        for output in enabled {
            if self.running.contains_key(&output.id) {
                continue;
            }
            let Some(pipeline) = self.encoders.get(&output.encoder_profile_id) else {
                continue;
            };
            let status = Arc::new(OutputStatus::default());
            let (release, release_requests) = mpsc::channel(1);
            let task = match sink_config(output)? {
                SinkConfig::Icecast(icecast) => {
                    let info = pipeline.info.clone();
                    let sink = match adopted.remove(&output.id) {
                        Some(connection) => {
                            tracing::info!(output = %output.name, "going on with the connection handed over");
                            IcecastSink::adopt(icecast, info, connection)
                        }
                        None => IcecastSink::new(icecast, info),
                    }
                    .with_relay(self.relay.clone());
                    self.runtime.spawn(sink::run(
                        sink,
                        output.name.clone(),
                        pipeline.audio.clone(),
                        self.metadata_for(output),
                        Arc::clone(&status),
                        release_requests,
                    ))
                }
            };
            self.running.insert(
                output.id,
                RunningOutput {
                    config: output.clone(),
                    status,
                    task,
                    release,
                },
            );
        }
        self.config = config.clone();
        Ok(())
    }

    /// Hands nothing on to the encoders after the master bus' frame
    /// `frame` (counted from the start): the last audio of an engine
    /// handing over to another.
    pub fn stop_after(&self, frame: u64) {
        self.fanout.limit.store(frame, Ordering::Relaxed);
    }

    /// Stops every encoder and output, for another engine to take over:
    /// the encoders encode what they were given and flush, then each output
    /// sends what was encoded and gives up its connection. Returns, per
    /// output ID, where its connection will come (`None`: it had none to
    /// give). The configuration is kept.
    pub fn release(&mut self) -> Vec<(u64, oneshot::Receiver<Option<HandedConnection>>)> {
        // The encoders stop, but keep their streams open: an output must not
        // see its stream end before it is asked for its connection.
        for pipeline in self.encoders.values_mut() {
            pipeline.finish(&self.fanout);
        }
        self.running
            .drain()
            .filter_map(|(id, output)| {
                let (request, connection) = oneshot::channel();
                match output.release.try_send(request) {
                    Ok(()) => Some((id, connection)),
                    Err(_) => {
                        output.task.abort();
                        None
                    }
                }
            })
            .collect()
    }

    /// A copy of the master bus (interleaved stereo at 48 kHz), for a local
    /// monitor. Dropped when the receiver falls behind, like an encoder's.
    pub fn tap(&self) -> Receiver<Arc<[f32]>> {
        let (sender, receiver) = std::sync::mpsc::sync_channel(ENCODER_QUEUE);
        self.fanout.add(u64::MAX, sender);
        receiver
    }

    pub fn config(&self) -> &OutputsConfig {
        &self.config
    }

    pub fn snapshot(&self) -> Vec<OutputSnapshot> {
        self.config
            .outputs
            .iter()
            .map(|output| {
                let running = self.running.get(&output.id);
                OutputSnapshot {
                    id: output.id,
                    name: output.name.clone(),
                    enabled: output.enabled,
                    state: running.map(|running| running.status.state()),
                    bytes_sent: running.map_or(0, |running| {
                        running.status.bytes_sent.load(Ordering::Relaxed)
                    }),
                    dropped_chunks: running.map_or(0, |running| {
                        running.status.dropped_chunks.load(Ordering::Relaxed)
                    }),
                }
            })
            .collect()
    }

    /// Metadata for an output: the station's, or nothing when it is disabled
    /// for this output.
    fn metadata_for(&self, output: &OutputConfig) -> watch::Receiver<NowPlaying> {
        if output.metadata_enabled {
            self.metadata.clone()
        } else {
            watch::channel(NowPlaying::default()).1
        }
    }
}

impl Drop for Outputs {
    fn drop(&mut self) {
        for output in self.running.values() {
            output.task.abort();
        }
        for (_, pipeline) in self.encoders.drain() {
            pipeline.stop(&self.fanout);
        }
    }
}

enum SinkConfig {
    Icecast(IcecastConfig),
}

fn sink_config(output: &OutputConfig) -> Result<SinkConfig, ConfigError> {
    match output.kind {
        OutputKind::Icecast => Ok(SinkConfig::Icecast(IcecastConfig::parse(
            &output.name,
            &output.config,
        )?)),
        kind => Err(ConfigError::Output {
            output: output.name.clone(),
            reason: format!("{kind:?} outputs arrive in M11; only Icecast is available"),
        }),
    }
}

/// Seconds since the Unix epoch, for status reports.
pub fn unix_seconds(time: SystemTime) -> u64 {
    time.duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |since| since.as_secs())
}
