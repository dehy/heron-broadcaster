// SPDX-License-Identifier: AGPL-3.0-or-later

//! The real-time pacing clock: the audio thread.
//!
//! No sound card drives the engine. This thread renders one block every
//! 10 ms against the system's monotonic clock, a little ahead of real time,
//! and hands it to the outputs through a lock-free ring buffer. Deadlines are
//! computed from the number of frames rendered since the start, so timing
//! errors never accumulate: over 24 h the stream holds exactly 24 h of audio.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crate::mixer::{BLOCK_FRAMES, CHANNELS, Mixer, SAMPLE_RATE};

/// How far ahead of real time blocks are rendered, to absorb scheduling
/// jitter downstream.
const LEAD: Duration = Duration::from_millis(50);
/// Past this delay (machine suspended, process stopped), the clock gives up
/// catching up and starts again from now instead of bursting audio.
const MAX_LATE: Duration = Duration::from_secs(1);

/// Health of the pacing clock, read by the metrics.
#[derive(Debug, Default)]
pub struct ClockStatus {
    /// Highest lateness of a block since the last read, in microseconds.
    pub max_late_us: AtomicU64,
    /// Times the clock gave up catching up.
    pub resets: AtomicU64,
    /// Blocks dropped because the outputs did not keep up.
    pub dropped_blocks: AtomicU64,
}

pub struct AudioThread {
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl AudioThread {
    /// Starts rendering `mixer` into `master` in real time.
    pub fn spawn(
        mut mixer: Mixer,
        mut master: rtrb::Producer<f32>,
        status: Arc<ClockStatus>,
    ) -> std::io::Result<Self> {
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = Arc::clone(&stop);
        let handle = std::thread::Builder::new()
            .name("heron-audio".to_owned())
            .spawn(move || {
                let mut block = vec![0.0_f32; BLOCK_FRAMES * CHANNELS];
                let mut start = Instant::now();
                let mut frames = 0_u64;
                while !thread_stop.load(Ordering::Relaxed) {
                    let due = start + frames_to_duration(frames);
                    let now = Instant::now();
                    if let Some(wait) = due.checked_duration_since(now + LEAD) {
                        std::thread::sleep(wait);
                    } else if let Some(late) = now.checked_duration_since(due) {
                        if late > MAX_LATE {
                            status.resets.fetch_add(1, Ordering::Relaxed);
                            start = now - frames_to_duration(frames);
                        }
                        let late_us = u64::try_from(late.as_micros()).unwrap_or(u64::MAX);
                        status.max_late_us.fetch_max(late_us, Ordering::Relaxed);
                    }

                    mixer.render(&mut block);
                    frames += BLOCK_FRAMES as u64;
                    push_block(&mut master, &block, &status);
                }
            })?;
        Ok(Self {
            stop,
            handle: Some(handle),
        })
    }
}

impl Drop for AudioThread {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

fn frames_to_duration(frames: u64) -> Duration {
    let nanos = u128::from(frames) * 1_000_000_000 / u128::from(SAMPLE_RATE);
    Duration::from_nanos(u64::try_from(nanos).unwrap_or(u64::MAX))
}

/// Hands a block to the outputs, or drops it whole when they are behind.
fn push_block(master: &mut rtrb::Producer<f32>, block: &[f32], status: &ClockStatus) {
    match master.write_chunk(block.len()) {
        Ok(mut chunk) => {
            let (first, second) = chunk.as_mut_slices();
            let split = first.len();
            first.copy_from_slice(&block[..split]);
            second.copy_from_slice(&block[split..]);
            chunk.commit_all();
        }
        Err(_) => {
            status.dropped_blocks.fetch_add(1, Ordering::Relaxed);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deadlines_do_not_accumulate_rounding_errors() {
        // 24 h of 10 ms blocks lands exactly on 24 h.
        let blocks = 24 * 3_600 * 100_u64;
        assert_eq!(
            frames_to_duration(blocks * BLOCK_FRAMES as u64),
            Duration::from_secs(24 * 3_600)
        );
        assert_eq!(frames_to_duration(1), Duration::from_nanos(20_833));
    }

    #[test]
    fn renders_in_real_time() {
        let (mixer, _handle) = Mixer::new(BLOCK_FRAMES);
        let (producer, consumer) = rtrb::RingBuffer::new(SAMPLE_RATE as usize * CHANNELS * 2);
        let status = Arc::new(ClockStatus::default());
        let started = Instant::now();
        let thread = AudioThread::spawn(mixer, producer, Arc::clone(&status)).unwrap();
        std::thread::sleep(Duration::from_millis(500));
        let samples = consumer.slots();
        let elapsed = started.elapsed().as_secs_f64();
        drop(thread);

        // 500 ms of audio plus the lead, give or take a block or two.
        let seconds = samples as f64 / (f64::from(SAMPLE_RATE) * CHANNELS as f64);
        let expected = elapsed + LEAD.as_secs_f64();
        assert!(
            (seconds - expected).abs() < 0.05,
            "rendered {seconds:.3} s in {elapsed:.3} s"
        );
        assert_eq!(status.dropped_blocks.load(Ordering::Relaxed), 0);
    }
}
