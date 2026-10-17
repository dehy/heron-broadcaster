// SPDX-License-Identifier: AGPL-3.0-or-later

//! Encoder profiles and outputs, as stored in `encoder_profiles` and
//! `outputs` and pushed to the engine. See `docs/outputs.md`.
//!
//! Encoder options and sink settings stay JSON here, like in the database:
//! `heron-outputs` reads and validates them when it builds the pipeline.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EncoderKind {
    Mp3,
    Aac,
    Opus,
    Vorbis,
    ExternalFfmpeg,
}

/// One encoding of the master bus. Outputs sharing a profile share its
/// encoder: the audio is encoded once.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EncoderProfile {
    pub id: u64,
    pub name: String,
    pub encoder: EncoderKind,
    pub bitrate_kbps: Option<u16>,
    pub sample_rate: u32,
    pub channels: u8,
    /// Encoder-specific options (MP3: `{"mode": "cbr", "quality": 2}`).
    #[serde(default)]
    pub options: serde_json::Value,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutputKind {
    Icecast,
    ShoutcastV1,
    ShoutcastV2,
    Hls,
    File,
    Pipe,
}

/// One destination of an encoded stream.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OutputConfig {
    pub id: u64,
    pub name: String,
    pub kind: OutputKind,
    pub encoder_profile_id: u64,
    /// Sink settings (Icecast: host, port, mount, credentials…).
    pub config: serde_json::Value,
    #[serde(default = "enabled")]
    pub metadata_enabled: bool,
    #[serde(default = "enabled")]
    pub enabled: bool,
}

fn enabled() -> bool {
    true
}

/// Every encoder profile and output of the station.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct OutputsConfig {
    pub encoder_profiles: Vec<EncoderProfile>,
    pub outputs: Vec<OutputConfig>,
}
