// SPDX-License-Identifier: AGPL-3.0-or-later

//! Separation rules and their relaxation (`docs/rotation.md`).

use serde::{Deserialize, Serialize};

/// Separation windows, in minutes; 0 turns a rule off.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(default, deny_unknown_fields)]
pub struct Windows {
    /// Do not play the same album within.
    pub album_min: u32,
    /// Do not play the same artist (or a similar one) within.
    pub artist_min: u32,
    /// Do not play the same title (any version) within.
    pub title_min: u32,
    /// Do not play the same track within.
    pub track_min: u32,
}

impl Default for Windows {
    fn default() -> Self {
        Self {
            album_min: 60,
            artist_min: 120,
            title_min: 240,
            track_min: 240,
        }
    }
}

/// Separation windows overridden by a category or a clock slot; missing
/// ones are inherited.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct SeparationOverrides {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub album_min: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artist_min: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title_min: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub track_min: Option<u32>,
}

impl SeparationOverrides {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// Last relaxation step: the fallback category's least recently played
/// track, without rules.
pub const LAST_STEP: u8 = 10;

/// First step taken in the fallback category.
pub const FALLBACK_STEP: u8 = 9;

/// The longest any window can be: one week.
pub const MAX_WINDOW_MIN: u32 = 7 * 24 * 60;

impl Windows {
    pub const OFF: Self = Self {
        album_min: 0,
        artist_min: 0,
        title_min: 0,
        track_min: 0,
    };

    pub fn with(self, overrides: &SeparationOverrides) -> Self {
        Self {
            album_min: overrides.album_min.unwrap_or(self.album_min),
            artist_min: overrides.artist_min.unwrap_or(self.artist_min),
            title_min: overrides.title_min.unwrap_or(self.title_min),
            track_min: overrides.track_min.unwrap_or(self.track_min),
        }
    }

    /// The windows at a relaxation step, from these as configured. Steps 9
    /// and 10 apply to the fallback category's windows.
    pub fn at_step(self, step: u8) -> Self {
        let mut windows = self;
        if step >= 1 {
            windows.album_min /= 2;
        }
        if step >= 2 {
            windows.album_min = 0;
        }
        if step >= 3 {
            windows.artist_min /= 2;
        }
        if step >= 4 {
            windows.artist_min = 0;
        }
        if step >= 5 {
            windows.title_min /= 2;
        }
        if step >= 6 {
            windows.title_min = 0;
        }
        if step >= 7 {
            windows.track_min = self.track_min / 2;
        }
        if step >= 8 {
            windows.track_min = self.track_min / 4;
        }
        if step >= LAST_STEP {
            windows.track_min = 0;
        }
        windows
    }

    pub fn longest_min(&self) -> u32 {
        self.album_min
            .max(self.artist_min)
            .max(self.title_min)
            .max(self.track_min)
    }

    /// Checks the windows are within a week.
    pub fn check(&self) -> Result<(), String> {
        if self.longest_min() > MAX_WINDOW_MIN {
            return Err(format!(
                "separation windows are at most {MAX_WINDOW_MIN} minutes"
            ));
        }
        Ok(())
    }
}

/// Windows across stations (`docs/stations.md#rules-across-stations`):
/// another station's play or planned track holds a track back within them,
/// before or after it. 0 turns a rule off; all are off by default.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(default, deny_unknown_fields)]
pub struct AcrossWindows {
    /// The same artist (or a similar one).
    pub artist_min: u32,
    /// The same title (any version).
    pub title_min: u32,
    /// The same track.
    pub track_min: u32,
}

impl AcrossWindows {
    pub const OFF: Self = Self {
        artist_min: 0,
        title_min: 0,
        track_min: 0,
    };

    /// Relaxed with the station's own rule of the same kind: artist halved
    /// at step 3 and dropped at 4, title at 5 and 6, track at 7 and 8.
    pub fn at_step(self, step: u8) -> Self {
        let mut windows = self;
        if step >= 3 {
            windows.artist_min /= 2;
        }
        if step >= 4 {
            windows.artist_min = 0;
        }
        if step >= 5 {
            windows.title_min /= 2;
        }
        if step >= 6 {
            windows.title_min = 0;
        }
        if step >= 7 {
            windows.track_min /= 2;
        }
        if step >= 8 {
            windows.track_min = 0;
        }
        windows
    }

    pub fn longest_min(&self) -> u32 {
        self.artist_min.max(self.title_min).max(self.track_min)
    }

    pub fn is_off(&self) -> bool {
        self.longest_min() == 0
    }

    pub fn check(&self) -> Result<(), String> {
        if self.longest_min() > MAX_WINDOW_MIN {
            return Err(format!(
                "windows across stations are at most {MAX_WINDOW_MIN} minutes"
            ));
        }
        Ok(())
    }
}

/// What a relaxation step relaxes, for people.
pub fn describe_step(step: u8) -> &'static str {
    match step {
        0 => "all rules as configured",
        1 => "album separation halved",
        2 => "album separation dropped",
        3 => "artist separation halved",
        4 => "artist separation dropped",
        5 => "title separation halved",
        6 => "title separation dropped",
        7 => "track separation halved",
        8 => "track separation quartered",
        9 => "fallback category, track separation at a quarter",
        _ => "fallback category, least recently played, no rules",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relaxation_follows_the_documented_table() {
        let windows = Windows::default();
        let at = |step| {
            let w = windows.at_step(step);
            (w.album_min, w.artist_min, w.title_min, w.track_min)
        };
        assert_eq!(at(0), (60, 120, 240, 240));
        assert_eq!(at(1), (30, 120, 240, 240));
        assert_eq!(at(2), (0, 120, 240, 240));
        assert_eq!(at(3), (0, 60, 240, 240));
        assert_eq!(at(4), (0, 0, 240, 240));
        assert_eq!(at(5), (0, 0, 120, 240));
        assert_eq!(at(6), (0, 0, 0, 240));
        assert_eq!(at(7), (0, 0, 0, 120));
        assert_eq!(at(8), (0, 0, 0, 60));
        assert_eq!(at(9), (0, 0, 0, 60));
        assert_eq!(at(10), (0, 0, 0, 0));
    }

    #[test]
    fn windows_across_stations_relax_with_the_station_s_own() {
        let across = AcrossWindows {
            artist_min: 30,
            title_min: 60,
            track_min: 120,
        };
        let at = |step| {
            let w = across.at_step(step);
            (w.artist_min, w.title_min, w.track_min)
        };
        assert_eq!(at(0), (30, 60, 120));
        assert_eq!(at(2), (30, 60, 120));
        assert_eq!(at(3), (15, 60, 120));
        assert_eq!(at(4), (0, 60, 120));
        assert_eq!(at(5), (0, 30, 120));
        assert_eq!(at(6), (0, 0, 120));
        assert_eq!(at(7), (0, 0, 60));
        assert_eq!(at(8), (0, 0, 0));
        assert!(AcrossWindows::default().is_off());
    }

    #[test]
    fn overrides_replace_some_windows() {
        let windows = Windows::default().with(&SeparationOverrides {
            track_min: Some(90),
            artist_min: Some(0),
            ..SeparationOverrides::default()
        });
        assert_eq!(
            windows,
            Windows {
                album_min: 60,
                artist_min: 0,
                title_min: 240,
                track_min: 90,
            }
        );
        let parsed: SeparationOverrides = serde_json::from_str(r#"{"title_min": 30}"#).unwrap();
        assert_eq!(parsed.title_min, Some(30));
        assert!(!parsed.is_empty());
    }
}
