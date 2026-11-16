// SPDX-License-Identifier: AGPL-3.0-or-later

//! The live monitor (`docs/audio-engine.md#live-monitor`): the master bus,
//! before and after the AGC chain, for the web UI, a fraction of a second
//! behind the air.
//!
//! While someone listens (a `WatchMonitor` stream is open), the audio thread
//! writes each block, raw and processed, into a ring
//! ([`crate::mixer::MixerStatus::monitoring`]). This thread reads it, cuts
//! it into chunks of 20 ms converted to 16 bits, and hands them to every
//! stream. Nobody listening, it clears the flag: the audio thread is back
//! to doing nothing for it.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::Duration;

use tokio::sync::broadcast;

use crate::mixer::{CHANNELS, MONITOR_CHANNELS, MixerStatus};

/// Frames of a chunk: 20 ms at 48 kHz, an Opus frame.
pub const CHUNK_FRAMES: usize = 960;
/// Chunks a stream may fall behind by before it loses some: 1 s.
const STREAM_CHUNKS: usize = 50;
/// How often the thread reads the ring.
const POLL: Duration = Duration::from_millis(5);
/// How often it checks for a listener when there is none.
const IDLE_POLL: Duration = Duration::from_millis(20);

/// 20 ms of the master bus, raw and processed: 16-bit little-endian,
/// interleaved stereo, 48 kHz.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MonitorChunk {
    /// Its first frame, counted from when the monitor was turned on.
    pub frame: u64,
    pub raw: Vec<u8>,
    pub processed: Vec<u8>,
}

pub struct LiveMonitor {
    chunks: broadcast::Sender<Arc<MonitorChunk>>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl LiveMonitor {
    /// Starts the thread reading `ring`, the mixer's monitor ring.
    pub fn start(ring: rtrb::Consumer<f32>, status: Arc<MixerStatus>) -> std::io::Result<Self> {
        let (chunks, _) = broadcast::channel(STREAM_CHUNKS);
        let stop = Arc::new(AtomicBool::new(false));
        let thread = std::thread::Builder::new()
            .name("heron-live-monitor".to_owned())
            .spawn({
                let chunks = chunks.clone();
                let stop = Arc::clone(&stop);
                move || run(ring, &status, &chunks, &stop)
            })?;
        Ok(Self {
            chunks,
            stop,
            thread: Some(thread),
        })
    }

    /// The chunks from now on: the audio thread taps the master bus while
    /// one receiver at least is alive.
    pub fn subscribe(&self) -> broadcast::Receiver<Arc<MonitorChunk>> {
        self.chunks.subscribe()
    }
}

impl Drop for LiveMonitor {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn run(
    mut ring: rtrb::Consumer<f32>,
    status: &MixerStatus,
    chunks: &broadcast::Sender<Arc<MonitorChunk>>,
    stop: &AtomicBool,
) {
    let mut pending: Vec<f32> = Vec::with_capacity(CHUNK_FRAMES * MONITOR_CHANNELS);
    let mut frame = 0_u64;
    while !stop.load(Ordering::Relaxed) {
        let listening = chunks.receiver_count() > 0;
        if listening != status.monitoring.load(Ordering::Relaxed) {
            // What is left from the last time, or the last blocks.
            drain(&mut ring);
            pending.clear();
            frame = 0;
            status.monitoring.store(listening, Ordering::Relaxed);
        }
        if !listening {
            drain(&mut ring);
            std::thread::sleep(IDLE_POLL);
            continue;
        }
        while let Ok(read) = ring.read_chunk(ring.slots().min(pending.capacity() - pending.len())) {
            if read.is_empty() {
                break;
            }
            let (first, second) = read.as_slices();
            pending.extend_from_slice(first);
            pending.extend_from_slice(second);
            read.commit_all();
            if pending.len() == pending.capacity() {
                // Nobody left to hear it: no harm.
                let _ = chunks.send(Arc::new(chunk(frame, &pending)));
                frame += CHUNK_FRAMES as u64;
                pending.clear();
            }
        }
        std::thread::sleep(POLL);
    }
    status.monitoring.store(false, Ordering::Relaxed);
}

fn drain(ring: &mut rtrb::Consumer<f32>) {
    if let Ok(read) = ring.read_chunk(ring.slots()) {
        read.commit_all();
    }
}

/// The chunk of `samples`, frames of raw left and right then processed left
/// and right.
fn chunk(frame: u64, samples: &[f32]) -> MonitorChunk {
    let mut raw = Vec::with_capacity(samples.len() / MONITOR_CHANNELS * CHANNELS * 2);
    let mut processed = Vec::with_capacity(raw.capacity());
    for frame in samples.as_chunks::<MONITOR_CHANNELS>().0 {
        for sample in &frame[..CHANNELS] {
            raw.extend_from_slice(&to_i16(*sample).to_le_bytes());
        }
        for sample in &frame[CHANNELS..] {
            processed.extend_from_slice(&to_i16(*sample).to_le_bytes());
        }
    }
    MonitorChunk {
        frame,
        raw,
        processed,
    }
}

fn to_i16(sample: f32) -> i16 {
    // In range after the clamp: the cast cannot wrap.
    #[allow(clippy::cast_possible_truncation)]
    let value = (sample.clamp(-1.0, 1.0) * f32::from(i16::MAX)).round() as i16;
    value
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_chunk_splits_raw_and_processed() {
        let samples: Vec<f32> = (0..CHUNK_FRAMES)
            .flat_map(|_| [0.5, -0.5, 1.0, -2.0])
            .collect();
        let chunk = chunk(960, &samples);
        assert_eq!(chunk.frame, 960);
        assert_eq!(chunk.raw.len(), CHUNK_FRAMES * 4);
        assert_eq!(chunk.processed.len(), CHUNK_FRAMES * 4);
        assert_eq!(i16::from_le_bytes([chunk.raw[0], chunk.raw[1]]), 16_384);
        assert_eq!(i16::from_le_bytes([chunk.raw[2], chunk.raw[3]]), -16_384);
        assert_eq!(
            i16::from_le_bytes([chunk.processed[0], chunk.processed[1]]),
            i16::MAX
        );
        // Over full scale: clamped.
        assert_eq!(
            i16::from_le_bytes([chunk.processed[2], chunk.processed[3]]),
            -i16::MAX
        );
    }

    #[test]
    fn the_tap_is_on_only_while_someone_listens() {
        let (mut producer, consumer) = rtrb::RingBuffer::new(CHUNK_FRAMES * MONITOR_CHANNELS * 4);
        let status = Arc::new(MixerStatus::default());
        let monitor = LiveMonitor::start(consumer, Arc::clone(&status)).unwrap();
        std::thread::sleep(Duration::from_millis(60));
        assert!(!status.monitoring.load(Ordering::Relaxed));
        let mut chunks = monitor.subscribe();
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        while !status.monitoring.load(Ordering::Relaxed) {
            assert!(
                std::time::Instant::now() < deadline,
                "the tap never turned on"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
        for _ in 0..CHUNK_FRAMES {
            for sample in [0.25, 0.25, 0.5, 0.5] {
                producer.push(sample).unwrap();
            }
        }
        let chunk = loop {
            match chunks.try_recv() {
                Ok(chunk) => break chunk,
                Err(_) => {
                    assert!(std::time::Instant::now() < deadline, "no chunk");
                    std::thread::sleep(Duration::from_millis(5));
                }
            }
        };
        assert_eq!(chunk.frame, 0);
        assert_eq!(i16::from_le_bytes([chunk.raw[0], chunk.raw[1]]), 8_192);
        assert_eq!(
            i16::from_le_bytes([chunk.processed[0], chunk.processed[1]]),
            16_384
        );
        drop(chunks);
        while status.monitoring.load(Ordering::Relaxed) {
            assert!(
                std::time::Instant::now() < deadline + Duration::from_secs(1),
                "the tap stayed on"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}
