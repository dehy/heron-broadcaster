// SPDX-License-Identifier: AGPL-3.0-or-later

// Words for rotation: days, hours, rules, methods and separation windows.

import type { Method, Rule, SeparationOverrides, Target, Windows } from "../api";

export const DAYS = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"] as const;

/** `1` (Monday) to `7` (Sunday). */
export function dayName(day: number): string {
  return DAYS[day - 1] ?? `Day ${day}`;
}

/** `06:00`; 24 is midnight at the end of the day. */
export function hourLabel(hour: number): string {
  return `${String(hour % 24).padStart(2, "0")}:00`;
}

/** `Mon–Fri`, `Sat, Sun`, `every day`. */
export function daysLabel(days: number[]): string {
  const sorted = [...new Set(days)].filter((d) => d >= 1 && d <= 7).sort((a, b) => a - b);
  if (sorted.length === 7) {
    return "every day";
  }
  const runs: number[][] = [];
  for (const day of sorted) {
    const run = runs[runs.length - 1];
    if (run && run[run.length - 1] === day - 1) {
      run.push(day);
    } else {
      runs.push([day]);
    }
  }
  return runs
    .map((run) => {
      const first = dayName(run[0] ?? 1);
      const last = dayName(run[run.length - 1] ?? 1);
      if (run.length >= 3) {
        return `${first}–${last}`;
      }
      return run.map(dayName).join(", ");
    })
    .join(", ");
}

const METHODS: Record<Method, string> = {
  least_recently_played: "Least recently played",
  random: "Random",
  weighted: "Weighted",
};

export function methodLabel(method: Method): string {
  return METHODS[method];
}

export const METHOD_OPTIONS = (Object.keys(METHODS) as Method[]).map((value) => ({
  value,
  label: METHODS[value],
}));

const dateFormat = new Intl.DateTimeFormat(undefined, { dateStyle: "medium", timeZone: "UTC" });
const dayMonthFormat = new Intl.DateTimeFormat(undefined, {
  day: "numeric",
  month: "long",
  timeZone: "UTC",
});

function dateLabel(date: string, everyYear: boolean): string {
  const parsed = new Date(`${date}T00:00:00Z`);
  if (Number.isNaN(parsed.getTime())) {
    return date;
  }
  return (everyYear ? dayMonthFormat : dateFormat).format(parsed);
}

/** A rule for people: `Banned Mon–Fri 06:00–10:00`. */
export function describeRule(rule: Rule, categoryName?: (id: number) => string | undefined): string {
  switch (rule.kind) {
    case "daypart": {
      const verb = rule.mode === "ban" ? "Banned" : "Only";
      const hours =
        rule.start_hour === 0 && rule.end_hour === 24
          ? "all day"
          : `${hourLabel(rule.start_hour)}–${hourLabel(rule.end_hour)}`;
      return `${verb} ${daysLabel(rule.days)}, ${hours}`;
    }
    case "date_range": {
      const verb = rule.mode === "ban" ? "Banned" : "Only";
      const range = `${dateLabel(rule.start, rule.every_year)} to ${dateLabel(rule.end, rule.every_year)}`;
      return `${verb} from ${range}${rule.every_year ? ", every year" : ""}`;
    }
    case "not_after": {
      const name = categoryName?.(rule.category_id) ?? `category ${rule.category_id}`;
      return `Never right after ${name}`;
    }
  }
}

export function targetKind(target: Target): string {
  return { track: "Track", artist: "Artist", category: "Category" }[target.kind];
}

const WINDOW_NAMES: [keyof Windows, string][] = [
  ["album_min", "album"],
  ["artist_min", "artist"],
  ["title_min", "title"],
  ["track_min", "track"],
];

export const WINDOW_FIELDS = WINDOW_NAMES;

function minutes(value: number): string {
  if (value === 0) {
    return "off";
  }
  if (value % 60 === 0) {
    return `${value / 60} h`;
  }
  return `${value} min`;
}

/** The windows overridden: `track 1 h 30, artist off`; empty when none. */
export function describeOverrides(overrides: SeparationOverrides | undefined): string {
  if (!overrides) {
    return "";
  }
  return WINDOW_NAMES.filter(([key]) => overrides[key] !== undefined && overrides[key] !== null)
    .map(([key, name]) => `${name} ${minutes(overrides[key] ?? 0)}`)
    .join(", ");
}

/** Windows as applied: `album 1 h · artist 2 h · title 4 h · track 4 h`. */
export function describeWindows(windows: Windows): string {
  return WINDOW_NAMES.map(([key, name]) => `${name} ${minutes(windows[key])}`).join(" · ");
}
