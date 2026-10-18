// SPDX-License-Identifier: AGPL-3.0-or-later

//! Shared types: tracks, cue points, fade curves, settings and library tasks.

pub mod file;
pub mod library;
pub mod output;
pub mod playout;
pub mod settings;

pub use file::{FileStamp, LibraryPath};
pub use output::{EncoderKind, EncoderProfile, OutputConfig, OutputKind, OutputsConfig};
pub use playout::{
    CrossfadeSettings, FadeCurve, LoudnessSettings, PlaySource, PlayoutSettings,
    SilenceWatchdogSettings, TrackOverrides, TrackPlayout, Transition, plan_transition,
};
pub use settings::{AnalysisSettings, LibrarySettings};
