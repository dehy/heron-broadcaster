// SPDX-License-Identifier: AGPL-3.0-or-later

//! The live monitor (`/api/v1/monitor`): the station's sound as a WebSocket,
//! a fraction of a second behind the air, before or after the processing
//! (`docs/api.md#live-monitor`).
//!
//! The engine's stream is read once per station for all its listeners
//! ([`crate::engine::EngineLink::listen_monitor`]); each listener has its
//! own Opus encoder, so switching taps keeps its state: no gap, no click.

use axum::extract::Query;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;
use utoipa::{IntoParams, ToSchema};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::app::AppState;
use crate::auth::{CurrentUser, Role};
use crate::engine::{MonitorFeed, MonitorListener};
use crate::error::{ApiError, ErrorBody};
use crate::stations::Station;

/// Listeners of one station at most.
pub const MAX_LISTENERS: usize = 8;
const SAMPLE_RATE: u32 = 48_000;
const CHANNELS: u8 = 2;
const FRAME_MS: u32 = 20;
/// Frames of one chunk: 20 ms at 48 kHz.
const CHUNK_FRAMES: usize = 960;
const OPUS_BITRATE: u32 = 256_000;
/// Room for one Opus packet: 256 kbit/s is 640 bytes per 20 ms, VBR peaks
/// stay well under this.
const MAX_PACKET: usize = 4_000;

/// The sound sent.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum MonitorCodec {
    /// Opus, stereo, 48 kHz, 256 kbit/s, one packet per 20 ms.
    #[default]
    Opus,
    /// 16-bit little-endian PCM, interleaved stereo, 960 frames per message.
    Pcm,
}

/// Where in the chain the sound is taken.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum MonitorTap {
    /// After the processing (the AGC chain): what the encoders get.
    #[default]
    Processed,
    /// The master bus before the processing.
    Raw,
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct MonitorQuery {
    /// `opus` (the default) or `pcm`.
    pub codec: Option<MonitorCodec>,
    /// `processed` (the default) or `raw`.
    pub tap: Option<MonitorTap>,
}

/// The text messages of the monitor; the sound comes as binary messages.
#[derive(Debug, PartialEq, Serialize, ToSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MonitorMessage {
    /// First, before any sound.
    Ready {
        codec: MonitorCodec,
        sample_rate: u32,
        channels: u8,
        frame_ms: u32,
        /// With Opus only.
        #[serde(skip_serializing_if = "Option::is_none")]
        bitrate: Option<u32>,
        tap: MonitorTap,
    },
    /// The next sound is from this tap.
    Tap { tap: MonitorTap },
    /// Chunks of 20 ms lost: the engine's or the API's backlog was full.
    Lost { chunks: u64 },
    /// The engine stopped, or started again, answering.
    Engine { reachable: bool },
    /// The engine has no live monitor: it is older than it. The socket closes.
    TooOld,
}

/// What a client sends: the tap to switch to.
#[derive(Debug, Deserialize)]
struct Switch {
    tap: MonitorTap,
}

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(monitor))
}

/// Opens the live monitor: a text [`MonitorMessage`] `ready`, then the sound
/// as binary messages, one per 20 ms. Operators and admins; at most 8
/// listeners per station.
#[utoipa::path(get, path = "/monitor", tag = "live",
    params(MonitorQuery),
    responses((status = 101, description = "WebSocket of the sound and MonitorMessage", body = MonitorMessage),
              (status = 403, body = ErrorBody),
              (status = 429, description = "8 listeners already", body = ErrorBody),
              (status = "4XX", body = ErrorBody)))]
async fn monitor(
    user: CurrentUser,
    station: Station,
    Query(query): Query<MonitorQuery>,
    upgrade: WebSocketUpgrade,
) -> Response {
    if let Err(error) = user.require(Role::Operator) {
        return error.into_response();
    }
    let Some(listener) = station.engine.listen_monitor(MAX_LISTENERS) else {
        let body = ErrorBody {
            error: "too_many_requests".to_owned(),
            message: format!("{MAX_LISTENERS} people already listen to this station's monitor"),
        };
        return (StatusCode::TOO_MANY_REQUESTS, axum::Json(body)).into_response();
    };
    let codec = query.codec.unwrap_or_default();
    let tap = query.tap.unwrap_or_default();
    let encoder = match codec {
        MonitorCodec::Opus => match opus_encoder() {
            Ok(encoder) => Some(encoder),
            Err(error) => return ApiError::Internal(format!("opus: {error}")).into_response(),
        },
        MonitorCodec::Pcm => None,
    };
    upgrade.on_upgrade(move |socket| async move {
        metrics::gauge!("heron_api_monitor_listeners").increment(1.0);
        if let Err(error) = serve(socket, listener, encoder, tap).await {
            tracing::debug!(%error, "monitor listener gone");
        }
        metrics::gauge!("heron_api_monitor_listeners").decrement(1.0);
    })
}

fn opus_encoder() -> Result<opus::Encoder, opus::Error> {
    let mut encoder = opus::Encoder::new(
        SAMPLE_RATE,
        opus::Channels::Stereo,
        opus::Application::Audio,
    )?;
    encoder.set_bitrate(opus::Bitrate::Bits(OPUS_BITRATE as i32))?;
    encoder.set_vbr(true)?;
    encoder.set_vbr_constraint(true)?;
    Ok(encoder)
}

async fn send(socket: &mut WebSocket, message: &MonitorMessage) -> Result<(), axum::Error> {
    let json = serde_json::to_string(message).map_err(axum::Error::new)?;
    socket.send(Message::Text(json.into())).await
}

/// One listener: its tap, its encoder, what it was last told.
struct Listener {
    encoder: Option<opus::Encoder>,
    tap: MonitorTap,
    reachable: bool,
    samples: Vec<i16>,
    packet: Vec<u8>,
}

impl Listener {
    fn new(encoder: Option<opus::Encoder>, tap: MonitorTap) -> Self {
        Self {
            encoder,
            tap,
            reachable: true,
            samples: Vec::with_capacity(CHUNK_FRAMES * usize::from(CHANNELS)),
            packet: vec![0; MAX_PACKET],
        }
    }

    fn ready(&self) -> MonitorMessage {
        MonitorMessage::Ready {
            codec: if self.encoder.is_some() {
                MonitorCodec::Opus
            } else {
                MonitorCodec::Pcm
            },
            sample_rate: SAMPLE_RATE,
            channels: CHANNELS,
            frame_ms: FRAME_MS,
            bitrate: self.encoder.as_ref().map(|_| OPUS_BITRATE),
            tap: self.tap,
        }
    }

    /// The binary message for a chunk of the engine: the tap listened to,
    /// as PCM or encoded.
    fn audio(&mut self, chunk: &heron_proto::engine::MonitorChunk) -> Result<Vec<u8>, opus::Error> {
        let pcm = match self.tap {
            MonitorTap::Processed => &chunk.processed,
            MonitorTap::Raw => &chunk.raw,
        };
        let Some(encoder) = &mut self.encoder else {
            return Ok(pcm.clone());
        };
        self.samples.clear();
        self.samples.extend(
            pcm.chunks_exact(2)
                .map(|bytes| i16::from_le_bytes([bytes[0], bytes[1]])),
        );
        // A short chunk would be refused by Opus: pad it with silence.
        self.samples.resize(CHUNK_FRAMES * usize::from(CHANNELS), 0);
        let length = encoder.encode(&self.samples, &mut self.packet)?;
        Ok(self.packet[..length].to_vec())
    }
}

async fn serve(
    mut socket: WebSocket,
    mut listener: MonitorListener,
    encoder: Option<opus::Encoder>,
    tap: MonitorTap,
) -> Result<(), axum::Error> {
    let mut state = Listener::new(encoder, tap);
    send(&mut socket, &state.ready()).await?;
    loop {
        tokio::select! {
            feed = listener.feed.recv() => match feed {
                Ok(MonitorFeed::Chunk(chunk)) => {
                    if !state.reachable {
                        state.reachable = true;
                        send(&mut socket, &MonitorMessage::Engine { reachable: true }).await?;
                    }
                    if chunk.lost > 0 {
                        send(&mut socket, &MonitorMessage::Lost { chunks: u64::from(chunk.lost) }).await?;
                    }
                    let audio = state.audio(&chunk).map_err(axum::Error::new)?;
                    socket.send(Message::Binary(audio.into())).await?;
                }
                Ok(MonitorFeed::Unreachable) => {
                    if state.reachable {
                        state.reachable = false;
                        send(&mut socket, &MonitorMessage::Engine { reachable: false }).await?;
                    }
                }
                Ok(MonitorFeed::Unsupported) => {
                    send(&mut socket, &MonitorMessage::TooOld).await?;
                    return socket.send(Message::Close(None)).await;
                }
                Err(broadcast::error::RecvError::Lagged(chunks)) => {
                    send(&mut socket, &MonitorMessage::Lost { chunks }).await?;
                }
                Err(broadcast::error::RecvError::Closed) => return Ok(()),
            },
            incoming = socket.recv() => match incoming {
                Some(Ok(Message::Text(text))) => {
                    // Anything else than a switch is ignored.
                    if let Ok(Switch { tap }) = serde_json::from_str(&text) {
                        state.tap = tap;
                        send(&mut socket, &MonitorMessage::Tap { tap }).await?;
                    }
                }
                Some(Ok(Message::Close(_))) | None | Some(Err(_)) => return Ok(()),
                Some(Ok(_)) => {}
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use heron_proto::engine::MonitorChunk;

    /// A chunk whose raw tap is all `raw` and processed tap all `processed`.
    fn chunk(raw: i16, processed: i16) -> MonitorChunk {
        let tap = |value: i16| {
            std::iter::repeat_n(value.to_le_bytes(), CHUNK_FRAMES * 2)
                .flatten()
                .collect::<Vec<u8>>()
        };
        MonitorChunk {
            frame: 0,
            raw: tap(raw),
            processed: tap(processed),
            lost: 0,
        }
    }

    #[test]
    fn pcm_is_the_tap_listened_to() {
        let mut listener = Listener::new(None, MonitorTap::Processed);
        let chunk = chunk(1000, -2000);
        assert_eq!(listener.audio(&chunk).unwrap(), chunk.processed);
        listener.tap = MonitorTap::Raw;
        let audio = listener.audio(&chunk).unwrap();
        assert_eq!(audio.len(), 3_840);
        assert_eq!(audio, chunk.raw);
    }

    /// 20 ms of a 440 Hz sine on the raw tap, silence on the processed one.
    fn sine(start: usize) -> MonitorChunk {
        let raw = (start..start + CHUNK_FRAMES)
            .flat_map(|frame| {
                let phase = frame as f64 * 440.0 * std::f64::consts::TAU / f64::from(SAMPLE_RATE);
                let sample = (phase.sin() * 8000.0) as i16;
                [sample, sample]
            })
            .flat_map(i16::to_le_bytes)
            .collect::<Vec<u8>>();
        MonitorChunk {
            frame: start as u64,
            processed: vec![0; raw.len()],
            raw,
            lost: 0,
        }
    }

    #[test]
    fn opus_packets_decode_to_the_tap_listened_to() {
        let mut listener = Listener::new(Some(opus_encoder().unwrap()), MonitorTap::Raw);
        let mut decoder = opus::Decoder::new(SAMPLE_RATE, opus::Channels::Stereo).unwrap();
        let mut decoded = vec![0_i16; CHUNK_FRAMES * 2];
        let rms = |samples: &[i16]| {
            (samples.iter().map(|&s| f64::from(s).powi(2)).sum::<f64>() / samples.len() as f64)
                .sqrt()
        };
        // Give the encoder a few chunks to settle, then look at the last.
        let mut frame = 0;
        let mut bytes = 0;
        for _ in 0..10 {
            let packet = listener.audio(&sine(frame)).unwrap();
            bytes += packet.len();
            assert_eq!(
                decoder.decode(&packet, &mut decoded, false).unwrap(),
                CHUNK_FRAMES
            );
            frame += CHUNK_FRAMES;
        }
        // 256 kbit/s is 640 bytes per 20 ms, give or take the VBR.
        assert!((400..900).contains(&(bytes / 10)), "{} bytes", bytes / 10);
        // The sine's RMS is 8000 / sqrt(2), about 5657.
        assert!((rms(&decoded) - 5657.0).abs() < 600.0, "{}", rms(&decoded));
        listener.tap = MonitorTap::Processed;
        for _ in 0..10 {
            let packet = listener.audio(&sine(frame)).unwrap();
            decoder.decode(&packet, &mut decoded, false).unwrap();
            frame += CHUNK_FRAMES;
        }
        assert!(rms(&decoded) < 50.0, "{}", rms(&decoded));
    }

    #[test]
    fn messages_are_as_documented() {
        let ready = Listener::new(Some(opus_encoder().unwrap()), MonitorTap::Processed).ready();
        assert_eq!(
            serde_json::to_value(&ready).unwrap(),
            serde_json::json!({"type": "ready", "codec": "opus", "sample_rate": 48000,
                               "channels": 2, "frame_ms": 20, "bitrate": 256000,
                               "tap": "processed"})
        );
        let ready = Listener::new(None, MonitorTap::Raw).ready();
        assert_eq!(
            serde_json::to_value(&ready).unwrap(),
            serde_json::json!({"type": "ready", "codec": "pcm", "sample_rate": 48000,
                               "channels": 2, "frame_ms": 20, "tap": "raw"})
        );
        assert_eq!(
            serde_json::to_string(&MonitorMessage::Lost { chunks: 3 }).unwrap(),
            r#"{"type":"lost","chunks":3}"#
        );
        assert_eq!(
            serde_json::to_string(&MonitorMessage::Engine { reachable: false }).unwrap(),
            r#"{"type":"engine","reachable":false}"#
        );
        assert_eq!(
            serde_json::to_string(&MonitorMessage::TooOld).unwrap(),
            r#"{"type":"too_old"}"#
        );
        let switch: Switch = serde_json::from_str(r#"{"tap": "raw"}"#).unwrap();
        assert_eq!(switch.tap, MonitorTap::Raw);
    }
}
