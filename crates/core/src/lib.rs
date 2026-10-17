// SPDX-License-Identifier: AGPL-3.0-or-later

//! Shared types: tracks, cue points, fade curves and settings.

pub mod file;
pub mod output;
pub mod playout;
pub mod settings;

pub use file::{FileStamp, LibraryPath};
pub use output::{EncoderKind, EncoderProfile, OutputConfig, OutputKind, OutputsConfig};
pub use playout::{
    CrossfadeSettings, FadeCurve, LoudnessSettings, PlayoutSettings, TrackOverrides, TrackPlayout,
    Transition, plan_transition,
};
pub use settings::AnalysisSettings;
