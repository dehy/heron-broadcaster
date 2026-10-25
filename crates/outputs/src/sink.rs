// SPDX-License-Identifier: AGPL-3.0-or-later

//! The `OutputSink` trait, and the driver that keeps one output running:
//! connecting, streaming, pushing metadata and reconnecting with backoff.

use std::future::Future;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use bytes::Bytes;
use tokio::sync::{broadcast, mpsc, oneshot, watch};

/// Now-playing metadata pushed to stream servers.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NowPlaying {
    pub artist: Option<String>,
    pub title: Option<String>,
}

impl NowPlaying {
    /// `Artist - Title`, the usual stream title. `None` when nothing is known.
    pub fn stream_title(&self) -> Option<String> {
        match (self.artist.as_deref(), self.title.as_deref()) {
            (Some(artist), Some(title)) => Some(format!("{artist} - {title}")),
            (None, Some(only)) | (Some(only), None) => Some(only.to_owned()),
            (None, None) => None,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SinkError {
    #[error("cannot connect: {0}")]
    Connect(std::io::Error),
    #[error("the server refused the stream: {0}")]
    Refused(String),
    #[error("connection lost: {0}")]
    Io(#[from] std::io::Error),
    #[error("timed out {0}")]
    Timeout(&'static str),
}

/// A destination for one encoded stream. Implementations only speak their
/// protocol; reconnection and isolation are handled by [`run`].
pub trait OutputSink: Send + 'static {
    /// Opens the stream.
    fn connect(&mut self) -> impl Future<Output = Result<(), SinkError>> + Send;
    /// Sends encoded audio. An error closes the stream and reconnects.
    fn send(&mut self, data: &[u8]) -> impl Future<Output = Result<(), SinkError>> + Send;
    /// Pushes now-playing metadata. An error is logged, the stream goes on.
    fn update_metadata(
        &mut self,
        now_playing: &NowPlaying,
    ) -> impl Future<Output = Result<(), SinkError>> + Send;
    /// Closes the stream.
    fn disconnect(&mut self);
    /// Gives up the open connection, still streaming, for another engine to
    /// go on writing to it. `None` when the sink has none to give.
    fn take_connection(&mut self) -> Option<std::net::TcpStream> {
        None
    }
}

/// A request to an output to stop and give up its connection
/// ([`OutputSink::take_connection`]), answered once the audio already
/// encoded has been sent.
pub type ReleaseRequest = oneshot::Sender<Option<std::net::TcpStream>>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OutputState {
    Connecting,
    Connected {
        since: SystemTime,
    },
    /// Waiting before the next attempt.
    Reconnecting {
        error: String,
        attempt: u32,
    },
}

/// Live state of one output, shared with the API and the metrics.
#[derive(Debug)]
pub struct OutputStatus {
    state: Mutex<OutputState>,
    pub bytes_sent: AtomicU64,
    /// Audio chunks lost because the output did not keep up.
    pub dropped_chunks: AtomicU64,
}

impl Default for OutputStatus {
    fn default() -> Self {
        Self {
            state: Mutex::new(OutputState::Connecting),
            bytes_sent: AtomicU64::new(0),
            dropped_chunks: AtomicU64::new(0),
        }
    }
}

impl OutputStatus {
    pub fn state(&self) -> OutputState {
        self.state
            .lock()
            .map_or(OutputState::Connecting, |state| state.clone())
    }

    fn set(&self, state: OutputState) {
        if let Ok(mut current) = self.state.lock() {
            *current = state;
        }
    }
}

/// Reconnection delays: 1 s, doubling up to 60 s.
fn backoff(attempt: u32) -> Duration {
    Duration::from_secs(1 << attempt.min(6)).min(Duration::from_secs(60))
}

/// How [`stream`] ended.
enum Streamed {
    Failed(SinkError),
    Released(ReleaseRequest),
}

/// Runs `sink` until the task is aborted or a request on `release` asks for
/// its connection: connects, streams the audio published on `audio`, pushes
/// metadata changes, and reconnects with exponential backoff after any error.
pub async fn run<S: OutputSink>(
    mut sink: S,
    name: String,
    audio: broadcast::Sender<Bytes>,
    mut metadata: watch::Receiver<NowPlaying>,
    status: Arc<OutputStatus>,
    mut release: mpsc::Receiver<ReleaseRequest>,
) {
    let labels = [("output", name.clone())];
    let connected = metrics::gauge!("heron_output_connected", &labels);
    let bytes = metrics::counter!("heron_output_bytes_total", &labels);
    let reconnects = metrics::counter!("heron_output_reconnects_total", &labels);
    let dropped = metrics::counter!("heron_output_dropped_chunks_total", &labels);

    let mut attempt = 0_u32;
    loop {
        status.set(OutputState::Connecting);
        let error = match sink.connect().await {
            Ok(()) => {
                // Subscribe once connected: the stream starts with fresh audio.
                let mut receiver = audio.subscribe();
                tracing::info!(output = %name, "output connected");
                status.set(OutputState::Connected {
                    since: SystemTime::now(),
                });
                connected.set(1.0);
                attempt = 0;
                let current = metadata.borrow_and_update().clone();
                push_metadata(&mut sink, &name, &current).await;
                let streamed = stream(
                    &mut sink,
                    &name,
                    &mut receiver,
                    &mut metadata,
                    &mut release,
                    &status,
                    &bytes,
                    &dropped,
                )
                .await;
                connected.set(0.0);
                match streamed {
                    Streamed::Failed(error) => {
                        sink.disconnect();
                        error
                    }
                    Streamed::Released(request) => {
                        // What was encoded before the request goes out first.
                        while let Ok(chunk) = receiver.try_recv() {
                            if sink.send(&chunk).await.is_err() {
                                break;
                            }
                            status
                                .bytes_sent
                                .fetch_add(chunk.len() as u64, Ordering::Relaxed);
                            bytes.increment(chunk.len() as u64);
                        }
                        let connection = sink.take_connection();
                        tracing::info!(output = %name, handed_over = connection.is_some(), "output released");
                        let _ = request.send(connection);
                        return;
                    }
                }
            }
            Err(error) => error,
        };
        let delay = backoff(attempt);
        tracing::warn!(output = %name, %error, retry_in_s = delay.as_secs(), "output disconnected");
        status.set(OutputState::Reconnecting {
            error: error.to_string(),
            attempt,
        });
        reconnects.increment(1);
        attempt = attempt.saturating_add(1);
        tokio::select! {
            () = tokio::time::sleep(delay) => {}
            Some(request) = release.recv() => {
                // Not connected: nothing to hand over.
                let _ = request.send(None);
                return;
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn stream<S: OutputSink>(
    sink: &mut S,
    name: &str,
    receiver: &mut broadcast::Receiver<Bytes>,
    metadata: &mut watch::Receiver<NowPlaying>,
    release: &mut mpsc::Receiver<ReleaseRequest>,
    status: &OutputStatus,
    bytes: &metrics::Counter,
    dropped: &metrics::Counter,
) -> Streamed {
    // An output without metadata has no sender: stop watching once it is gone.
    let mut watch_metadata = true;
    loop {
        tokio::select! {
            chunk = receiver.recv() => match chunk {
                Ok(chunk) => {
                    if let Err(error) = sink.send(&chunk).await {
                        return Streamed::Failed(error);
                    }
                    status.bytes_sent.fetch_add(chunk.len() as u64, Ordering::Relaxed);
                    bytes.increment(chunk.len() as u64);
                }
                Err(broadcast::error::RecvError::Lagged(missed)) => {
                    status.dropped_chunks.fetch_add(missed, Ordering::Relaxed);
                    dropped.increment(missed);
                }
                Err(broadcast::error::RecvError::Closed) => {
                    return Streamed::Failed(SinkError::Io(std::io::Error::other("the encoder stopped")));
                }
            },
            Some(request) = release.recv() => return Streamed::Released(request),
            changed = metadata.changed(), if watch_metadata => {
                if changed.is_ok() {
                    let current = metadata.borrow_and_update().clone();
                    push_metadata(sink, name, &current).await;
                } else {
                    watch_metadata = false;
                }
            }
        }
    }
}

async fn push_metadata<S: OutputSink>(sink: &mut S, name: &str, now_playing: &NowPlaying) {
    if now_playing.stream_title().is_none() {
        return;
    }
    if let Err(error) = sink.update_metadata(now_playing).await {
        tracing::warn!(output = %name, %error, "metadata update failed");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_doubles_up_to_a_minute() {
        let delays: Vec<u64> = (0..9).map(|attempt| backoff(attempt).as_secs()).collect();
        assert_eq!(delays, [1, 2, 4, 8, 16, 32, 60, 60, 60]);
    }

    #[test]
    fn stream_title_joins_artist_and_title() {
        let both = NowPlaying {
            artist: Some("Yes".into()),
            title: Some("Close to the Edge".into()),
        };
        assert_eq!(both.stream_title().unwrap(), "Yes - Close to the Edge");
        let title = NowPlaying {
            title: Some("Station ID".into()),
            ..NowPlaying::default()
        };
        assert_eq!(title.stream_title().unwrap(), "Station ID");
        assert_eq!(NowPlaying::default().stream_title(), None);
    }
}
