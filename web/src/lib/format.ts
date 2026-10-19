// SPDX-License-Identifier: AGPL-3.0-or-later

// Formatting of times, durations and levels, in the current language.

import { locale, number, t } from "../i18n";

/** The current language's decimal separator: `.` in English, `,` in French. */
function decimal(): string {
  return new Intl.NumberFormat(locale()).formatToParts(1.5).find((part) => part.type === "decimal")?.value ?? ".";
}

/** `m:ss`, or `h:mm:ss` from an hour; tenths with `precise`. */
export function duration(ms: number | null | undefined, precise = false): string {
  if (ms === null || ms === undefined || !Number.isFinite(ms)) {
    return "–";
  }
  const negative = ms < 0;
  const total = Math.abs(ms);
  const tenths = Math.floor(total / 100) % 10;
  const seconds = Math.floor(total / 1000);
  const h = Math.floor(seconds / 3600);
  const m = Math.floor((seconds % 3600) / 60);
  const s = seconds % 60;
  const ss = String(s).padStart(2, "0");
  let text = h > 0 ? `${h}:${String(m).padStart(2, "0")}:${ss}` : `${m}:${ss}`;
  if (precise) {
    text += `${decimal()}${tenths}`;
  }
  return negative ? `-${text}` : text;
}

/**
 * Parses `m:ss.t`, `ss.t` or `h:mm:ss` into ms; `null` when invalid. A comma
 * works as the decimal separator too (`1:23,5`).
 */
export function parseDuration(text: string): number | null {
  const parts = text.trim().replace(",", ".").split(":");
  if (parts.length === 0 || parts.length > 3 || parts.some((part) => part.trim() === "")) {
    return null;
  }
  let seconds = 0;
  for (const part of parts) {
    const value = Number(part);
    if (!Number.isFinite(value) || value < 0) {
      return null;
    }
    seconds = seconds * 60 + value;
  }
  return Math.round(seconds * 1000);
}

/** A level in dB, with its sign. */
export function db(value: number | null | undefined, unit = "dB"): string {
  if (value === null || value === undefined || !Number.isFinite(value)) {
    return "–";
  }
  const text = number(value, {
    minimumFractionDigits: 1,
    maximumFractionDigits: 1,
    signDisplay: "exceptZero",
  });
  return `${text} ${unit}`;
}

// Times are shown in the station's time zone, like the schedule and the
// restrictions; in the browser's until the station's is known.
let zone: string | undefined;
let zoned: { key: string; formats: ReturnType<typeof formats> } | undefined;

function formats(language: string, timeZone?: string) {
  return {
    time: new Intl.DateTimeFormat(language, { timeStyle: "medium", timeZone }),
    dateTime: new Intl.DateTimeFormat(language, { dateStyle: "medium", timeStyle: "short", timeZone }),
    fullDate: new Intl.DateTimeFormat(language, { dateStyle: "full", timeZone }),
    relative: new Intl.RelativeTimeFormat(language, { numeric: "auto" }),
  };
}

/** The formats of the current language and zone, made again when either changes. */
function current() {
  const key = `${locale()} ${zone ?? ""}`;
  if (zoned?.key !== key) {
    zoned = { key, formats: formats(locale(), zone) };
  }
  return zoned.formats;
}

/** Shows times in `timeZone` (an IANA name) from now on; a name the browser does not know is ignored. */
export function setTimeZone(timeZone: string) {
  if (timeZone === zone) {
    return;
  }
  try {
    formats(locale(), timeZone);
    zone = timeZone;
  } catch {
    // Keep the current zone.
  }
}

/** Time of day of a Unix time in ms. */
export function clockTime(unixMs: number): string {
  return current().time.format(new Date(unixMs));
}

/** Date and time of a Unix time in ms. */
export function dateTime(unixMs: number | null | undefined): string {
  return unixMs === null || unixMs === undefined ? "–" : current().dateTime.format(new Date(unixMs));
}

/** The date of a Unix time in ms, written in full with its weekday. */
export function fullDate(unixMs: number): string {
  return current().fullDate.format(new Date(unixMs));
}

/** "5 minutes ago", "yesterday"… */
export function ago(unixMs: number | null | undefined, now = Date.now()): string {
  if (unixMs === null || unixMs === undefined) {
    return t("format.never");
  }
  const seconds = Math.round((unixMs - now) / 1000);
  const units: [Intl.RelativeTimeFormatUnit, number][] = [
    ["day", 86_400],
    ["hour", 3_600],
    ["minute", 60],
  ];
  for (const [unit, size] of units) {
    if (Math.abs(seconds) >= size) {
      return current().relative.format(Math.round(seconds / size), unit);
    }
  }
  return current().relative.format(seconds, "second");
}

const BYTE_UNITS = ["format.bytes", "format.kilobytes", "format.megabytes", "format.gigabytes", "format.terabytes"] as const;

/** File sizes and byte counts: `4.3 MB`, `4,3 Mo` in French. */
export function bytes(value: number | null | undefined): string {
  if (value === null || value === undefined) {
    return "–";
  }
  let size = value;
  let unit = 0;
  while (size >= 1000 && unit < BYTE_UNITS.length - 1) {
    size /= 1000;
    unit += 1;
  }
  const digits = unit === 0 ? 0 : 1;
  const text = number(size, { minimumFractionDigits: digits, maximumFractionDigits: digits });
  return t(BYTE_UNITS[unit] ?? "format.bytes", { size: text });
}

/** "Artist – Title", falling back on the file name. */
export function trackName(track: {
  artist?: string | null;
  title?: string | null;
  path: string;
}): string {
  const file = track.path.split("/").pop() ?? track.path;
  if (track.title && track.artist) {
    return `${track.artist} – ${track.title}`;
  }
  return track.title ?? file;
}
