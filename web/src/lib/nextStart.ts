// SPDX-License-Identifier: AGPL-3.0-or-later

// Where the next track starts, set like a cue point (docs/web.md › Track).

import type { TrackOverrides } from "../api";

/**
 * The overrides with the next track starting at `ms` (file timeline): a
 * fixed crossfade that long before the cue-out, crossfading back on.
 */
export function withNextStart(overrides: TrackOverrides, ms: number, cueOut: number): TrackOverrides {
  const { crossfade: _off, ...crossfade } = overrides.crossfade ?? {};
  return {
    ...overrides,
    crossfade: {
      ...crossfade,
      crossfade: null,
      mode: "fixed",
      fixed_ms: Math.max(0, Math.round((cueOut - Math.min(ms, cueOut)) / 100) * 100),
    },
  };
}
