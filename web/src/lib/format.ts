// SPDX-License-Identifier: AGPL-3.0-or-later

// Formatting of times, durations and levels.

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
    text += `.${tenths}`;
  }
  return negative ? `-${text}` : text;
}

/** Parses `m:ss.t`, `ss.t` or `h:mm:ss` into ms; `null` when invalid. */
export function parseDuration(text: string): number | null {
  const parts = text.trim().split(":");
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
  const sign = value > 0 ? "+" : "";
  return `${sign}${value.toFixed(1)} ${unit}`;
}

const timeFormat = new Intl.DateTimeFormat(undefined, { timeStyle: "medium" });
const dateTimeFormat = new Intl.DateTimeFormat(undefined, {
  dateStyle: "medium",
  timeStyle: "short",
});

/** Time of day of a Unix time in ms. */
export function clockTime(unixMs: number): string {
  return timeFormat.format(new Date(unixMs));
}

/** Date and time of a Unix time in ms. */
export function dateTime(unixMs: number | null | undefined): string {
  return unixMs === null || unixMs === undefined ? "–" : dateTimeFormat.format(new Date(unixMs));
}

// The interface is in English: so are relative times.
const relativeFormat = new Intl.RelativeTimeFormat("en", { numeric: "auto" });

/** "5 minutes ago", "yesterday"… */
export function ago(unixMs: number | null | undefined, now = Date.now()): string {
  if (unixMs === null || unixMs === undefined) {
    return "never";
  }
  const seconds = Math.round((unixMs - now) / 1000);
  const units: [Intl.RelativeTimeFormatUnit, number][] = [
    ["day", 86_400],
    ["hour", 3_600],
    ["minute", 60],
  ];
  for (const [unit, size] of units) {
    if (Math.abs(seconds) >= size) {
      return relativeFormat.format(Math.round(seconds / size), unit);
    }
  }
  return relativeFormat.format(seconds, "second");
}

/** File sizes and byte counts. */
export function bytes(value: number | null | undefined): string {
  if (value === null || value === undefined) {
    return "–";
  }
  const units = ["B", "kB", "MB", "GB", "TB"];
  let size = value;
  let unit = 0;
  while (size >= 1000 && unit < units.length - 1) {
    size /= 1000;
    unit += 1;
  }
  return `${size.toFixed(unit === 0 ? 0 : 1)} ${units[unit]}`;
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

/** "1 track", "3 tracks": `count` with `word`, plural with an s. */
export function count(n: number, word: string, plural = `${word}s`): string {
  return `${n.toLocaleString()} ${n === 1 ? word : plural}`;
}
