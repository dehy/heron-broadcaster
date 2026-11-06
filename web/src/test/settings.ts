// SPDX-License-Identifier: AGPL-3.0-or-later

// A station's settings as heron-api answers for a new station: the defaults of
// `heron_core` (`AgcSettings`, `EqSettings`, `PlayoutSettings`).

import type { AgcSettings, EqSettings, PlayoutSettings } from "../api";

export const AGC_DEFAULTS = {
  "bypass_all": true,
  "gated_agc": {
    "enabled": false,
    "gate_lufs": -45.0,
    "max_gain_db": 6.0
  },
  "stereo_expander": {
    "enabled": false,
    "level": 50.0,
    "depth": 50.0,
    "threshold_db": -40.0
  },
  "bass_eq": {
    "enabled": false,
    "shape": "shelf",
    "frequency_hz": 100.0,
    "gain_db": 0.0
  },
  "five_band": {
    "enabled": false,
    "overdrive_db": 0.0,
    "crossovers_hz": [
      100.0,
      400.0,
      1600.0,
      6400.0
    ],
    "links": [
      false,
      false,
      false,
      false
    ],
    "bands": [
      {
        "mode": "compressor",
        "threshold_db": -20.0,
        "ratio": 3.0,
        "attack_ms": 20.0,
        "release_ms": 250.0,
        "hold_ms": 0.0,
        "gain_db": 0.0
      },
      {
        "mode": "compressor",
        "threshold_db": -20.0,
        "ratio": 3.0,
        "attack_ms": 20.0,
        "release_ms": 250.0,
        "hold_ms": 0.0,
        "gain_db": 0.0
      },
      {
        "mode": "compressor",
        "threshold_db": -20.0,
        "ratio": 3.0,
        "attack_ms": 20.0,
        "release_ms": 250.0,
        "hold_ms": 0.0,
        "gain_db": 0.0
      },
      {
        "mode": "compressor",
        "threshold_db": -20.0,
        "ratio": 3.0,
        "attack_ms": 20.0,
        "release_ms": 250.0,
        "hold_ms": 0.0,
        "gain_db": 0.0
      },
      {
        "mode": "compressor",
        "threshold_db": -20.0,
        "ratio": 3.0,
        "attack_ms": 20.0,
        "release_ms": 250.0,
        "hold_ms": 0.0,
        "gain_db": 0.0
      }
    ]
  },
  "dual_band": {
    "enabled": false,
    "crossover_hz": 200.0,
    "link": false,
    "bands": [
      {
        "mode": "compressor",
        "threshold_db": -20.0,
        "ratio": 3.0,
        "attack_ms": 20.0,
        "release_ms": 250.0,
        "hold_ms": 0.0,
        "gain_db": 0.0
      },
      {
        "mode": "compressor",
        "threshold_db": -20.0,
        "ratio": 3.0,
        "attack_ms": 20.0,
        "release_ms": 250.0,
        "hold_ms": 0.0,
        "gain_db": 0.0
      }
    ]
  },
  "final_stage": {
    "enabled": true,
    "mode": "limiter",
    "ceiling_db": -1.0,
    "release_ms": 100.0
  }
} as AgcSettings;

export const EQ_DEFAULTS = {
  "music": {
    "enabled": false,
    "gains_db": [
      0.0,
      0.0,
      0.0,
      0.0,
      0.0,
      0.0,
      0.0,
      0.0,
      0.0,
      0.0
    ]
  },
  "fx": {
    "enabled": false,
    "gains_db": [
      0.0,
      0.0,
      0.0,
      0.0,
      0.0,
      0.0,
      0.0,
      0.0,
      0.0,
      0.0
    ]
  }
} as EqSettings;

export const PLAYOUT_DEFAULTS = {
  "crossfade": {
    "fade_out": {
      "enabled": true,
      "curve": "linear",
      "time_ms": 2500,
      "level_pct": 0
    },
    "fade_in": {
      "enabled": false,
      "curve": "linear",
      "time_ms": 1000,
      "level_pct": 0
    },
    "mode": "auto",
    "fixed_ms": 2000,
    "min_track_ms": 20000,
    "skip_fade_ms": 1000
  },
  "loudness": {
    "enabled": true,
    "target_lufs": -16.0,
    "max_gain_db": 12.0,
    "peak_ceiling_dbtp": -1.0
  },
  "silence_watchdog": {
    "enabled": true,
    "timeout_ms": 20000
  }
} as PlayoutSettings;
