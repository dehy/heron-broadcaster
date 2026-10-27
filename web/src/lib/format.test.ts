// SPDX-License-Identifier: AGPL-3.0-or-later

import { afterEach, describe, expect, it } from "vitest";
import { locale, setLanguage } from "../i18n";
import {
  ago,
  bytes,
  clockTime,
  dateTime,
  db,
  duration,
  fullDate,
  parseDuration,
  setTimeZone,
  totalTime,
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

  it("formats long totals with hundredths and days", () => {
    expect(totalTime(0)).toBe("00:00:00.00");
    expect(totalTime(45_296_789)).toBe("12:34:56.78");
    expect(totalTime(2 * 86_400_000 + 3 * 3_600_000 + 4 * 60_000 + 5_000)).toBe("2 d 03:04:05.00");
    expect(totalTime(null)).toBe("–");
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
  // The language, and the machine's variant of it, decide how times are
  // written ("00:30:00", "12:30:00 AM"): compare with it, not with a pattern.
  const time = (timeZone: string, unixMs: number) =>
    new Intl.DateTimeFormat(locale(), { timeStyle: "medium", timeZone }).format(unixMs);

  it("writes times in the station's time zone", () => {
    setTimeZone("UTC");
    const first = fullDate(Date.UTC(2024, 0, 1, 12));
    const second = fullDate(Date.UTC(2024, 0, 2, 12));
    // 2024-01-01 23:30 UTC: already the 2nd in Paris, still the 1st in Montréal.
    const at = Date.UTC(2024, 0, 1, 23, 30);
    setTimeZone("Europe/Paris");
    expect(clockTime(at)).toBe(time("Europe/Paris", at));
    expect(fullDate(at)).toBe(second);
    setTimeZone("America/Montreal");
    expect(clockTime(at)).toBe(time("America/Montreal", at));
    expect(clockTime(at)).not.toBe(time("Europe/Paris", at));
    expect(fullDate(at)).toBe(first);
  });

  it("ignores a zone the browser does not know", () => {
    const at = Date.UTC(2024, 0, 1, 12);
    setTimeZone("UTC");
    const utc = clockTime(at);
    setTimeZone("Not/AZone");
    expect(clockTime(at)).toBe(utc);
  });
});

describe("in French", () => {
  afterEach(async () => {
    await setLanguage("en");
  });

  it("writes levels, sizes, durations and relative times the French way", async () => {
    await setLanguage("fr");
    expect(db(-1.5)).toMatch(/^[-−]1,5 dB$/);
    expect(db(3)).toBe("+3,0 dB");
    expect(bytes(4_321_000)).toBe("4,3 Mo");
    expect(bytes(999)).toBe("999 o");
    expect(duration(1_250, true)).toBe("0:01,2");
    const now = 1_700_000_000_000;
    expect(ago(now - 5 * 60_000, now)).toBe("il y a 5 minutes");
    expect(ago(now - 86_400_000, now)).toBe("hier");
    expect(ago(null, now)).toBe("jamais");
  });

  it("accepts a comma in durations", () => {
    expect(parseDuration("1:23,5")).toBe(83_500);
    expect(parseDuration("1:23.5")).toBe(83_500);
    expect(parseDuration("12,25")).toBe(12_250);
  });

  it("writes dates and times in French, in the same zone", async () => {
    setTimeZone("Europe/Paris");
    const at = Date.UTC(2024, 0, 1, 23, 30);
    await setLanguage("fr");
    const format = (options: Intl.DateTimeFormatOptions) =>
      new Intl.DateTimeFormat(locale(), { ...options, timeZone: "Europe/Paris" }).format(at);
    expect(clockTime(at)).toBe(format({ timeStyle: "medium" }));
    expect(fullDate(at)).toBe("mardi 2 janvier 2024");
    expect(dateTime(at)).toBe(format({ dateStyle: "medium", timeStyle: "short" }));
  });
});
