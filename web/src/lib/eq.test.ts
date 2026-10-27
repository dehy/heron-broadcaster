// SPDX-License-Identifier: AGPL-3.0-or-later

import { describe, expect, it } from "vitest";
import {
  CURVE_HEIGHT,
  CURVE_WIDTH,
  FREQUENCIES,
  arrowStep,
  bandLabel,
  curvePath,
  curveX,
  curveY,
  formatGain,
  presetWithGains,
  withGain,
  bandQ,
  filterGains,
  filtersResponseDb,
  isShaping,
  latestSender,
  responseDb,
  roundGain,
} from "./eq";

import fixture from "./eq.fixture.json";

const flat = () => FREQUENCIES.map(() => 0);

describe("the response curve", () => {
  it("is the engine's: the filter gains and curves of the shared fixture", () => {
    for (const { sliders_db, filter_gains_db, response_db } of fixture.cases) {
      const gains = filterGains(sliders_db);
      gains.forEach((gain, band) => expect(gain).toBeCloseTo(filter_gains_db[band] ?? NaN, 4));
      for (const [frequency, db] of response_db) {
        expect(filtersResponseDb(gains, frequency ?? 0)).toBeCloseTo(db ?? NaN, 4);
      }
    }
  });

  it("is flat for a flat EQ", () => {
    expect(filterGains(flat())).toEqual(flat());
    for (const frequency of [20, 100, 1_000, 10_000, 20_000]) {
      expect(responseDb(flat(), frequency)).toBeCloseTo(0, 9);
    }
  });

  it("meets each slider at its band", () => {
    const sliders = [0, 7, -8, 5, 7.5, 7, 6, 0, 0, 0];
    const gains = filterGains(sliders);
    FREQUENCIES.forEach((centre, band) => {
      expect(Math.abs(filtersResponseDb(gains, centre) - (sliders[band] ?? 0))).toBeLessThan(0.2);
    });
  });

  it("does not pile up between close bands", () => {
    const sliders = flat();
    sliders[7] = 12;
    sliders[8] = 12;
    // Between 12k and 14k, both at +12: about 12 dB, not 20.
    expect(responseDb(sliders, 13_000)).toBeLessThan(13);
  });

  it("widens the bands with the distance to their neighbours", () => {
    expect(bandQ(5)).toBeCloseTo(1.08, 2);
    expect(bandQ(8)).toBeCloseTo(6.95, 2);
    expect(bandQ(0)).toBeCloseTo(Math.SQRT1_2, 9);
  });
});

describe("bands", () => {
  it("have short labels", () => {
    expect(FREQUENCIES.map(bandLabel)).toEqual(["60", "170", "310", "600", "1k", "3k", "6k", "12k", "14k", "16k"]);
  });

  it("round to a tenth of a decibel within ±12", () => {
    expect(roundGain(3.26)).toBe(3.3);
    expect(roundGain(-0.04)).toBe(-0);
    expect(roundGain(15)).toBe(12);
    expect(roundGain(-40)).toBe(-12);
  });
});

describe("shaping", () => {
  it("needs the EQ on and a band away from 0 dB", () => {
    expect(isShaping(null)).toBe(false);
    expect(isShaping({ enabled: true, gains_db: flat() })).toBe(false);
    const gains = flat();
    gains[0] = 1;
    expect(isShaping({ enabled: false, gains_db: gains })).toBe(false);
    expect(isShaping({ enabled: true, gains_db: gains })).toBe(true);
  });
});

describe("the latest sender", () => {
  it("sends one value at a time, the latest", async () => {
    const sent: number[] = [];
    let release: () => void = () => {};
    const sender = latestSender((value: number) => {
      sent.push(value);
      return new Promise<void>((resolve) => {
        release = resolve;
      });
    }, 0);
    sender.push(1);
    sender.push(2);
    sender.push(3);
    await new Promise((resolve) => setTimeout(resolve, 5));
    // 1 is on its way; 2 was replaced by 3.
    expect(sent).toEqual([1]);
    expect(sender.active).toBe(true);
    release();
    await new Promise((resolve) => setTimeout(resolve, 5));
    expect(sent).toEqual([1, 3]);
    release();
    await new Promise((resolve) => setTimeout(resolve, 5));
    expect(sender.active).toBe(false);
  });

  it("waits between requests", async () => {
    const times: number[] = [];
    const sender = latestSender(async () => {
      times.push(Date.now());
    }, 50);
    sender.push(1);
    await new Promise((resolve) => setTimeout(resolve, 5));
    sender.push(2);
    await new Promise((resolve) => setTimeout(resolve, 100));
    expect(times).toHaveLength(2);
    expect((times[1] ?? 0) - (times[0] ?? 0)).toBeGreaterThanOrEqual(45);
  });

  it("reports errors and goes on", async () => {
    const errors: unknown[] = [];
    const sent: number[] = [];
    const sender = latestSender(async (value: number) => {
      sent.push(value);
      if (value === 1) {
        throw new Error("refused");
      }
    }, 0);
    sender.onError = (error) => errors.push(error);
    sender.push(1);
    await new Promise((resolve) => setTimeout(resolve, 5));
    sender.push(2);
    await new Promise((resolve) => setTimeout(resolve, 5));
    expect(errors).toHaveLength(1);
    expect(sent).toEqual([1, 2]);
  });
});

describe("the page's helpers", () => {
  it("format gains with their sign", () => {
    expect(formatGain(3.5)).toBe("+3.5");
    expect(formatGain(0)).toBe("0.0");
    expect(formatGain(-12)).toBe("-12.0");
  });

  it("change one band, rounded and within range", () => {
    const eq = { enabled: true, gains_db: flat() };
    const changed = withGain(eq, 2, 3.26);
    expect(changed.gains_db[2]).toBe(3.3);
    expect(changed.gains_db.filter((gain) => gain !== 0)).toHaveLength(1);
    expect(withGain(eq, 0, 40).gains_db[0]).toBe(12);
    // The EQ given is left as it was.
    expect(eq.gains_db[2]).toBe(0);
  });

  it("move a slider by half a decibel with the arrows", () => {
    expect(arrowStep("ArrowUp")).toBe(0.5);
    expect(arrowStep("ArrowRight")).toBe(0.5);
    expect(arrowStep("ArrowDown")).toBe(-0.5);
    expect(arrowStep("ArrowLeft")).toBe(-0.5);
    expect(arrowStep("Enter")).toBeUndefined();
  });

  it("find the preset loaded, by its gains", () => {
    const presets = [
      { id: 1, name: "Flat", gains_db: flat() },
      { id: 2, name: "Bass", gains_db: [3, ...flat().slice(1)] },
    ];
    expect(presetWithGains(presets, [3, ...flat().slice(1)])?.name).toBe("Bass");
    expect(presetWithGains(presets, flat())?.name).toBe("Flat");
    expect(presetWithGains(presets, [1, ...flat().slice(1)])).toBeUndefined();
    expect(presetWithGains(undefined, flat())).toBeUndefined();
  });

  it("draw the curve across 20 Hz to 20 kHz, clamped to ±15 dB", () => {
    expect(curveX(20)).toBeCloseTo(0, 9);
    expect(curveX(20_000)).toBeCloseTo(CURVE_WIDTH, 9);
    expect(curveY(0)).toBe(CURVE_HEIGHT / 2);
    expect(curveY(15)).toBe(0);
    expect(curveY(40)).toBe(0);
    expect(curveY(-40)).toBe(CURVE_HEIGHT);
    // Flat: a straight line through the middle, from end to end.
    const path = curvePath(flat(), 4, 0);
    expect(path).toBe(`M0,${CURVE_HEIGHT / 2}L150,70L300,70L450,70L600,70`);
    // A boost at 60 Hz lifts the start of the curve.
    const boosted = curvePath([12, ...flat().slice(1)], 4, 0);
    expect(Number(boosted.split(",")[1]?.split("L")[0])).toBeLessThan(CURVE_HEIGHT / 2);
  });
});
