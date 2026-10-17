// SPDX-License-Identifier: AGPL-3.0-or-later

//! Optional local monitor: plays the master bus on the machine's default
//! sound card (`cpal`). Built with the `monitor` feature.
//!
//! The monitor is fed like an encoder, from the fan-out, so a sound card
//! problem never reaches the audio thread or the outputs. It plays a little
//! behind the stream; it is for listening, not for timing.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::thread::JoinHandle;
use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample, StreamConfig};
use heron_dsp::StreamResampler;

use crate::mixer::{CHANNELS, SAMPLE_RATE};

/// Audio buffered for the sound card.
const BUFFER_SECONDS: f32 = 0.5;

#[derive(Debug, thiserror::Error)]
pub enum MonitorError {
    #[error("no sound card")]
    NoDevice,
    #[error("sound card: {0}")]
    Device(String),
    #[error("unsupported sample format {0}")]
    Format(SampleFormat),
    #[error(transparent)]
    Resample(#[from] heron_dsp::resample::Error),
}

pub struct Monitor {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Monitor {
    /// Starts playing `master` (interleaved stereo at 48 kHz) on the default
    /// sound card.
    pub fn start(master: Receiver<Arc<[f32]>>) -> Result<Self, MonitorError> {
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = Arc::clone(&stop);
        let (ready_tx, ready_rx) = std::sync::mpsc::channel();
        // `cpal` streams cannot move between threads on every platform: the
        // thread that opens the stream keeps it.
        let thread = std::thread::Builder::new()
            .name("heron-monitor".to_owned())
            .spawn(move || {
                let result = open().and_then(|(stream, producer, rate, channels)| {
                    let feeder = Feeder::new(producer, rate, channels)?;
                    Ok((stream, feeder))
                });
                match result {
                    Ok((stream, mut feeder)) => {
                        let _ = ready_tx.send(Ok(()));
                        feeder.run(&master, &thread_stop);
                        drop(stream);
                    }
                    Err(error) => {
                        let _ = ready_tx.send(Err(error));
                    }
                }
            })
            .map_err(|error| MonitorError::Device(error.to_string()))?;
        ready_rx
            .recv()
            .map_err(|_| MonitorError::Device("the monitor thread stopped".to_owned()))??;
        Ok(Self {
            stop,
            thread: Some(thread),
        })
    }
}

impl Drop for Monitor {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

type Opened = (cpal::Stream, rtrb::Producer<f32>, u32, usize);

fn open() -> Result<Opened, MonitorError> {
    let device = cpal::default_host()
        .default_output_device()
        .ok_or(MonitorError::NoDevice)?;
    let supported = device
        .default_output_config()
        .map_err(|error| MonitorError::Device(error.to_string()))?;
    let format = supported.sample_format();
    let config: StreamConfig = supported.into();
    let rate = config.sample_rate;
    let channels = usize::from(config.channels);
    let capacity = (rate as f32 * BUFFER_SECONDS) as usize * channels;
    let (producer, consumer) = rtrb::RingBuffer::new(capacity);
    let stream = match format {
        SampleFormat::F32 => build::<f32>(&device, &config, consumer),
        SampleFormat::I16 => build::<i16>(&device, &config, consumer),
        SampleFormat::I32 => build::<i32>(&device, &config, consumer),
        SampleFormat::U16 => build::<u16>(&device, &config, consumer),
        other => return Err(MonitorError::Format(other)),
    }?;
    stream
        .play()
        .map_err(|error| MonitorError::Device(error.to_string()))?;
    tracing::info!(rate, channels, "monitor output started");
    Ok((stream, producer, rate, channels))
}

fn build<T>(
    device: &cpal::Device,
    config: &StreamConfig,
    mut consumer: rtrb::Consumer<f32>,
) -> Result<cpal::Stream, MonitorError>
where
    T: SizedSample + FromSample<f32>,
{
    device
        .build_output_stream(
            *config,
            move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
                // Runs on the sound card's thread: no allocation, silence
                // when the monitor falls behind.
                for sample in data.iter_mut() {
                    *sample = T::from_sample(consumer.pop().unwrap_or(0.0));
                }
            },
            |error| tracing::warn!(%error, "monitor output error"),
            None,
        )
        .map_err(|error| MonitorError::Device(error.to_string()))
}

/// Converts the master bus to the sound card's format.
struct Feeder {
    producer: rtrb::Producer<f32>,
    resampler: StreamResampler,
    channels: usize,
    resampled: Vec<f32>,
}

impl Feeder {
    fn new(
        producer: rtrb::Producer<f32>,
        rate: u32,
        channels: usize,
    ) -> Result<Self, MonitorError> {
        Ok(Self {
            producer,
            resampler: StreamResampler::new(SAMPLE_RATE, rate, CHANNELS)?,
            channels,
            resampled: Vec::new(),
        })
    }

    fn run(&mut self, master: &Receiver<Arc<[f32]>>, stop: &AtomicBool) {
        while !stop.load(Ordering::Relaxed) {
            let pcm = match master.recv_timeout(Duration::from_millis(100)) {
                Ok(pcm) => pcm,
                Err(RecvTimeoutError::Timeout) => continue,
                Err(RecvTimeoutError::Disconnected) => break,
            };
            self.resampled.clear();
            if self.resampler.process(&pcm, &mut self.resampled).is_err() {
                continue;
            }
            for frame in self.resampled.as_chunks::<CHANNELS>().0 {
                if self.producer.slots() < self.channels {
                    // The sound card is behind: drop rather than wait.
                    break;
                }
                for channel in 0..self.channels {
                    let sample = match (self.channels, channel) {
                        (1, _) => 0.5 * (frame[0] + frame[1]),
                        (_, 0 | 1) => frame[channel],
                        _ => 0.0,
                    };
                    let _ = self.producer.push(sample);
                }
            }
        }
    }
}
