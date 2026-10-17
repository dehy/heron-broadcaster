// SPDX-License-Identifier: AGPL-3.0-or-later

import { describe, expect, it } from "vitest";
import { curveGain, incomingGain, outgoingGain } from "./fade";

describe("fade curves", () => {
  it("go from 0 to 1", () => {
    for (const curve of ["linear", "s_curve", "logarithmic", "exponential"] as const) {
      expect(curveGain(curve, 0)).toBeCloseTo(0);
      expect(curveGain(curve, 1)).toBeCloseTo(1);
    }
    expect(curveGain("s_curve", 0.5)).toBeCloseTo(0.5);
    expect(curveGain("logarithmic", 0.5)).toBeGreaterThan(0.5);
    expect(curveGain("exponential", 0.5)).toBeLessThan(0.5);
  });

  it("ramp the outgoing track down to its level", () => {
    const ramp = { start_ms: 1_000, length_ms: 1_000, curve: "linear" as const, level: 0.2 };
    expect(outgoingGain(ramp, 3_000, 500)).toBe(1);
    expect(outgoingGain(ramp, 3_000, 1_500)).toBeCloseTo(0.6);
    expect(outgoingGain(ramp, 3_000, 2_500)).toBeCloseTo(0.2);
    expect(outgoingGain(ramp, 3_000, 3_000)).toBe(0);
    expect(outgoingGain(null, 3_000, 2_999)).toBe(1);
  });

  it("ramp the incoming track up from its cue-in", () => {
    const ramp = { start_ms: 200, length_ms: 1_000, curve: "linear" as const, level: 0 };
    expect(incomingGain(ramp, 200, 100)).toBe(0);
    expect(incomingGain(ramp, 200, 700)).toBeCloseTo(0.5);
    expect(incomingGain(ramp, 200, 5_000)).toBe(1);
    expect(incomingGain(undefined, 0, 0)).toBe(1);
  });
});
