// SPDX-License-Identifier: AGPL-3.0-or-later

import { describe, expect, it } from "vitest";
import { byteDb, levelAt, levelText } from "./level";

describe("levels", () => {
  it("reads waveform bytes as dBFS", () => {
    expect(byteDb(255)).toBeCloseTo(0);
    expect(byteDb(0)).toBe(-72);
    expect(byteDb(128)).toBeCloseTo(-35.86, 2);
  });

  it("finds the level at a time", () => {
    const peaks = new Uint8Array([0, 128, 255]);
    expect(levelAt(peaks, 50, 0)).toBe(-72);
    expect(levelAt(peaks, 50, 120)).toBeCloseTo(0);
    expect(levelAt(peaks, 50, 150)).toBeNull();
    expect(levelAt(peaks, 50, -1)).toBeNull();
  });

  it("shows the floor as a bound", () => {
    expect(levelText(-72)).toMatch(/^≤ /);
    expect(levelText(-12.3)).toMatch(/12[.,]3 dBFS$/);
    expect(levelText(null)).toBe("–");
  });
});
