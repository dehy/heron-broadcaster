// SPDX-License-Identifier: AGPL-3.0-or-later

//! Track analysis: loudness, silence, cue and fade point detection.
//!
//! Runs in `heron-analyzer` when a track is imported, never in the engine.
//! The algorithm is described in `docs/analysis.md`.

mod analyzer;
mod decode;
#[cfg(feature = "mysql")]
pub mod jobs;

pub use analyzer::{Analysis, Analyzer, FRAME_MS};
pub use decode::analyze_file;

/// Version of the analysis algorithm. Bump it when a change to this crate
/// alters results, or lets files that failed before decode now: every track
/// is then re-analyzed, including those whose analysis failed.
pub const ALGORITHM_VERSION: u32 = 1;

/// Errors raised while analyzing a file.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("cannot read the file: {0}")]
    Io(#[from] std::io::Error),
    #[error("cannot decode the file: {0}")]
    Decode(#[from] symphonia::core::errors::Error),
    #[error("the file has no audio track")]
    NoAudioTrack,
    #[error("the file contains no audio samples")]
    Empty,
    #[error(
        "the decoded audio is invalid (true peak {true_peak_db:.0} dBTP): corrupt file or decoder bug"
    )]
    InvalidAudio { true_peak_db: f64 },
    #[error("the audio format changes in the middle of the file")]
    FormatChanged,
    #[error("unsupported audio format: {rate} Hz, {channels} channels")]
    UnsupportedFormat { rate: u32, channels: usize },
    #[error("loudness measurement failed: {0}")]
    Loudness(#[from] ebur128::Error),
    #[cfg(feature = "mysql")]
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
}

impl Error {
    /// Whether trying again later might succeed (the file was briefly
    /// unreadable, the database was down). Broken files are not retried.
    pub fn is_transient(&self) -> bool {
        match self {
            Self::Io(_) => true,
            #[cfg(feature = "mysql")]
            Self::Database(_) => true,
            _ => false,
        }
    }
}
