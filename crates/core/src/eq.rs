// SPDX-License-Identifier: AGPL-3.0-or-later

//! Deck EQ settings, stored under `eq` in the `settings` table and pushed to
//! the engine. See `docs/audio-engine.md#deck-eq`.

use serde::{Deserialize, Serialize};

/// Bands of a deck EQ: 60, 170, 310, 600, 1k, 3k, 6k, 12k, 14k, 16k Hz.
pub const EQ_BANDS: usize = 10;
/// Highest boost and deepest cut of a band, in dB.
pub const EQ_MAX_GAIN_DB: f32 = 12.0;

/// The two deck EQs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(default, deny_unknown_fields)]
pub struct EqSettings {
    /// Decks A and B: they take turns, so they share one EQ.
    pub music: DeckEq,
    /// The FX player.
    pub fx: DeckEq,
}

impl EqSettings {
    /// Name of the row in the `settings` table.
    pub const SETTINGS_NAME: &str = "eq";

    pub fn check(&self) -> Result<(), String> {
        self.music
            .check()
            .map_err(|error| format!("music: {error}"))?;
        self.fx.check().map_err(|error| format!("fx: {error}"))
    }
}

/// One deck EQ. Flat and bypassed by default.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(default, deny_unknown_fields)]
pub struct DeckEq {
    pub enabled: bool,
    /// Gain of each band in dB, from 60 Hz up, −12 to +12.
    pub gains_db: [f32; EQ_BANDS],
}

impl DeckEq {
    pub fn check(&self) -> Result<(), String> {
        check_gains(&self.gains_db)
    }

    /// Enabled, with a band away from 0 dB: it changes the sound.
    pub fn is_shaping(&self) -> bool {
        self.enabled && self.gains_db.iter().any(|gain| *gain != 0.0)
    }
}

/// Gains of a deck EQ or a preset: each from −12 to +12 dB.
pub fn check_gains(gains_db: &[f32; EQ_BANDS]) -> Result<(), String> {
    if gains_db
        .iter()
        .all(|gain| (-EQ_MAX_GAIN_DB..=EQ_MAX_GAIN_DB).contains(gain))
    {
        Ok(())
    } else {
        Err(format!(
            "gains_db are from -{EQ_MAX_GAIN_DB} to {EQ_MAX_GAIN_DB} dB"
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_flat_and_bypassed() {
        let settings: EqSettings = serde_json::from_str("{}").unwrap();
        assert_eq!(settings, EqSettings::default());
        assert!(!settings.music.enabled);
        assert!(!settings.music.is_shaping());
        assert_eq!(settings.check(), Ok(()));
    }

    #[test]
    fn gains_are_checked() {
        let mut settings = EqSettings::default();
        settings.fx.gains_db[3] = 12.5;
        assert!(settings.check().unwrap_err().starts_with("fx:"));
        settings.fx.gains_db[3] = f32::NAN;
        assert!(settings.check().is_err());
        settings.fx.gains_db[3] = -12.0;
        assert_eq!(settings.check(), Ok(()));
    }

    #[test]
    fn ten_gains_exactly() {
        let nine = r#"{"music": {"enabled": true, "gains_db": [0, 0, 0, 0, 0, 0, 0, 0, 0]}}"#;
        assert!(serde_json::from_str::<EqSettings>(nine).is_err());
    }

    #[test]
    fn shaping_needs_a_band_away_from_zero() {
        let mut eq = DeckEq {
            enabled: true,
            ..DeckEq::default()
        };
        assert!(!eq.is_shaping());
        eq.gains_db[0] = 1.5;
        assert!(eq.is_shaping());
        eq.enabled = false;
        assert!(!eq.is_shaping());
    }
}
