// SPDX-License-Identifier: AGPL-3.0-or-later

import { describe, expect, it } from "vitest";
import {
  ago,
  bytes,
  clockTime,
  count,
  db,
  duration,
  fullDate,
  parseDuration,
  setTimeZone,
  trackName,
} from "./format";

describe("duration", () => {
  it("formats minutes and hours", () => {
    expect(duration(0)).toBe("0:00");
    expect(duration(61_999)).toBe("1:01");
    expect(duration(3_723_000)).toBe("1:02:03");
    expect(duration(-5_000)).toBe("-0:05");
    expect(duration(1_250, true)).toBe("0:01.2");
    expect(duration(null)).toBe("–");
  });

  it("parses what it formats", () => {
    expect(parseDuration("1:01.5")).toBe(61_500);
    expect(parseDuration("12.25")).toBe(12_250);
    expect(parseDuration("1:02:03")).toBe(3_723_000);
    expect(parseDuration("")).toBeNull();
    expect(parseDuration("1::2")).toBeNull();
    expect(parseDuration("-3")).toBeNull();
    expect(parseDuration("abc")).toBeNull();
  });
});

describe("levels and sizes", () => {
  it("shows signs and units", () => {
    expect(db(3)).toBe("+3.0 dB");
    expect(db(-14.04, "LUFS")).toBe("-14.0 LUFS");
    expect(db(Number.NEGATIVE_INFINITY)).toBe("–");
    expect(bytes(999)).toBe("999 B");
    expect(bytes(4_321_000)).toBe("4.3 MB");
  });
});

describe("names and times", () => {
  it("counts", () => {
    expect(count(1, "move")).toBe("1 move");
    expect(count(3, "file")).toBe("3 files");
    expect(count(2, "entry", "entries")).toBe("2 entries");
  });

  it("names tracks", () => {
    expect(trackName({ artist: "Miles Davis", title: "So What", path: "a/b.flac" })).toBe(
      "Miles Davis – So What",
    );
    expect(trackName({ title: null, path: "a/b.flac" })).toBe("b.flac");
  });

  it("says how long ago", () => {
    const now = 1_700_000_000_000;
    expect(ago(now - 5 * 60_000, now)).toBe("5 minutes ago");
    expect(ago(null, now)).toBe("never");
  });
});

describe("time zone", () => {
  it("writes times in the station's time zone", () => {
    setTimeZone("UTC");
    const first = fullDate(Date.UTC(2024, 0, 1, 12));
    const second = fullDate(Date.UTC(2024, 0, 2, 12));
    // 2024-01-01 23:30 UTC: already the 2nd in Paris, still the 1st in Montréal.
    const at = Date.UTC(2024, 0, 1, 23, 30);
    setTimeZone("Europe/Paris");
    expect(clockTime(at)).toMatch(/^(0?0|12):30:00/);
    expect(fullDate(at)).toBe(second);
    setTimeZone("America/Montreal");
    expect(clockTime(at)).toMatch(/^(18|6):30:00/);
    expect(fullDate(at)).toBe(first);
  });

  it("ignores a zone the browser does not know", () => {
    setTimeZone("UTC");
    setTimeZone("Not/AZone");
    expect(clockTime(Date.UTC(2024, 0, 1, 12))).toMatch(/^12:00:00/);
  });
});
