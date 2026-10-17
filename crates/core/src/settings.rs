// SPDX-License-Identifier: AGPL-3.0-or-later

//! Station settings shared between services. Stored as JSON in the
//! `settings` table, one row per group.

use serde::{Deserialize, Serialize};

/// Parameters of the import-time analysis (Gap Killer and crossfade point
/// detection). Stored under the `analysis` settings name.
///
/// Levels are in dB **relative to the track's integrated loudness**, so the
/// same thresholds work for quiet and loud masters. See `docs/analysis.md`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AnalysisSettings {
    /// Audio quieter than this at the start of a track is trimmed.
    pub start_threshold_db: f64,
    /// Audio quieter than this at the end of a track is trimmed.
    pub end_threshold_db: f64,
    /// Absolute ceiling (dBFS) for what counts as silence: quiet but audible
    /// passages above it are never trimmed, whatever the track's loudness.
    pub silence_ceiling_dbfs: f64,
    /// Bursts of sound this short, surrounded by silence, are ignored.
    pub blip_max_ms: u32,
    /// Silence longer than this inside a track ends the track there.
    pub max_internal_silence_ms: u32,
    /// The crossfade starts after the last moment the track is louder than this.
    pub fade_trigger_db: f64,
    /// Shortest distance between the crossfade point and the end of the track.
    pub fade_min_ms: u32,
    /// Longest distance between the crossfade point and the end of the track.
    pub fade_max_ms: u32,
}

impl AnalysisSettings {
    /// Name of the row in the `settings` table.
    pub const SETTINGS_NAME: &'static str = "analysis";

    /// Fingerprint of these settings. Stored with each analysis, so changing a
    /// setting marks every track for re-analysis.
    pub fn fingerprint(&self) -> [u8; 32] {
        // Struct fields serialize in a fixed order, so the JSON is canonical.
        let json = serde_json::to_vec(self).unwrap_or_default();
        *blake3::hash(&json).as_bytes()
    }
}

impl Default for AnalysisSettings {
    fn default() -> Self {
        Self {
            start_threshold_db: -20.0,
            end_threshold_db: -24.0,
            silence_ceiling_dbfs: -50.0,
            blip_max_ms: 10,
            max_internal_silence_ms: 15_000,
            fade_trigger_db: -15.0,
            fade_min_ms: 150,
            fade_max_ms: 2_500,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_fields_take_defaults() {
        let settings: AnalysisSettings = serde_json::from_str(r#"{"blip_max_ms": 20}"#).unwrap();
        assert_eq!(settings.blip_max_ms, 20);
        assert_eq!(settings.end_threshold_db, -24.0);
    }

    #[test]
    fn fingerprint_changes_with_settings() {
        let default = AnalysisSettings::default();
        let changed = AnalysisSettings {
            fade_trigger_db: -12.0,
            ..AnalysisSettings::default()
        };
        assert_eq!(
            default.fingerprint(),
            AnalysisSettings::default().fingerprint()
        );
        assert_ne!(default.fingerprint(), changed.fingerprint());
    }
}
