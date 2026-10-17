// SPDX-License-Identifier: AGPL-3.0-or-later

//! Encoders and output sinks: the `OutputSink` trait, the MP3 encoder and the
//! Icecast sink, and the pipeline feeding several outputs at once.
//! Shoutcast, HLS, file recording, pipes and more encoders arrive in M10.
//!
//! See `docs/outputs.md`.

pub mod encoder;
pub mod icecast;
pub mod pipeline;
pub mod sink;

pub use pipeline::{OutputSnapshot, Outputs};
pub use sink::{NowPlaying, OutputSink, OutputState};

/// A configuration the engine cannot apply.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("encoder profile {profile:?}: {reason}")]
    Profile { profile: String, reason: String },
    #[error("output {output:?}: {reason}")]
    Output { output: String, reason: String },
}
