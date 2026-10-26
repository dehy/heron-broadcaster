// SPDX-License-Identifier: AGPL-3.0-or-later

import { describe, expect, it } from "vitest";
import { FREQUENCIES, bandLabel, isShaping, latestSender, responseDb, roundGain } from "./eq";

const flat = () => FREQUENCIES.map(() => 0);

describe("the response curve", () => {
  it("is flat for a flat EQ", () => {
    for (const frequency of [20, 100, 1_000, 10_000, 20_000]) {
      expect(responseDb(flat(), frequency)).toBeCloseTo(0, 9);
    }
  });

  it("peaks at a band's gain on its centre", () => {
    FREQUENCIES.forEach((centre, band) => {
      const gains = flat();
      gains[band] = 12;
      // Its neighbours are at 0 dB: they add nothing.
      expect(responseDb(gains, centre)).toBeCloseTo(12, 6);
    });
  });

  it("adds overlapping bands up", () => {
    const gains = flat();
    gains[7] = 12;
    gains[8] = 12;
    // Between 12k and 14k, both boost.
    expect(responseDb(gains, 13_000)).toBeGreaterThan(12);
  });
});

describe("bands", () => {
  it("have short labels", () => {
    expect(FREQUENCIES.map(bandLabel)).toEqual(["60", "170", "310", "600", "1k", "3k", "6k", "12k", "14k", "16k"]);
  });

  it("round to half a decibel within ±12", () => {
    expect(roundGain(3.26)).toBe(3.5);
    expect(roundGain(-0.2)).toBe(-0);
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
