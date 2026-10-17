// SPDX-License-Identifier: AGPL-3.0-or-later

// Gains of fades, as the engine computes them (heron-core's
// `FadeCurve::fade_in` and `Ramp`), to draw transitions.

import type { FadeCurve, Schemas } from "../api";

type Ramp = Schemas["RampView"];

/** Gain of a fade-in at `progress` (0 to 1). */
export function curveGain(curve: FadeCurve, progress: number): number {
  const x = Math.min(1, Math.max(0, progress));
  switch (curve) {
    case "linear":
      return x;
    case "s_curve":
      return 0.5 - 0.5 * Math.cos(Math.PI * x);
    case "logarithmic":
      return Math.log10(1 + 9 * x);
    case "exponential":
      return (10 ** x - 1) / 9;
  }
}

/**
 * Gain of the outgoing track at `ms` on its timeline: 1 before its fade,
 * down the ramp, 0 after its cue-out.
 */
export function outgoingGain(ramp: Ramp | null | undefined, cueOutMs: number, ms: number): number {
  if (ms >= cueOutMs) {
    return 0;
  }
  if (!ramp || ms <= ramp.start_ms) {
    return 1;
  }
  if (ms >= ramp.start_ms + ramp.length_ms) {
    return ramp.level;
  }
  const progress = (ms - ramp.start_ms) / ramp.length_ms;
  return ramp.level + (1 - ramp.level) * curveGain(ramp.curve, 1 - progress);
}

/** Gain of the incoming track at `ms` on its own timeline. */
export function incomingGain(ramp: Ramp | null | undefined, cueInMs: number, ms: number): number {
  if (ms < cueInMs) {
    return 0;
  }
  if (!ramp || ms >= ramp.start_ms + ramp.length_ms) {
    return 1;
  }
  const progress = Math.max(0, ms - ramp.start_ms) / ramp.length_ms;
  return ramp.level + (1 - ramp.level) * curveGain(ramp.curve, progress);
}
