// SPDX-License-Identifier: AGPL-3.0-or-later

// Listener statistics: the current counts, and the days of a period in
// station time (see docs/statistics.md).

import { useQuery } from "@tanstack/react-query";
import { api, unwrap } from "../api";

/** Each output's listeners at the last minute; read again every half minute. */
export function useListenersNow() {
  return useQuery({
    queryKey: ["listeners", "now"],
    queryFn: () => unwrap(api.GET("/api/v1/listeners/now")),
    refetchInterval: 30_000,
  });
}

/** `YYYY-MM-DD` of a Unix time in ms, in `timeZone` (the browser's when missing). */
export function dayOf(unixMs: number, timeZone?: string): string {
  // en-CA writes dates as YYYY-MM-DD.
  return new Intl.DateTimeFormat("en-CA", { timeZone, year: "numeric", month: "2-digit", day: "2-digit" }).format(
    new Date(unixMs),
  );
}

/** The day `count` days after `day` (before when negative). */
export function addDays(day: string, count: number): string {
  const [y, m, d] = day.split("-").map(Number);
  const date = new Date(Date.UTC(y ?? 1970, (m ?? 1) - 1, (d ?? 1) + count));
  return date.toISOString().slice(0, 10);
}

/** How far `timeZone` is ahead of UTC at a time, in ms. */
function offset(unixMs: number, timeZone?: string): number {
  const parts = Object.fromEntries(
    new Intl.DateTimeFormat("en-US", {
      timeZone,
      hourCycle: "h23",
      year: "numeric",
      month: "2-digit",
      day: "2-digit",
      hour: "2-digit",
      minute: "2-digit",
      second: "2-digit",
    })
      .formatToParts(new Date(unixMs))
      .map((part) => [part.type, Number(part.value)]),
  );
  const local = Date.UTC(parts.year ?? 1970, (parts.month ?? 1) - 1, parts.day ?? 1, parts.hour ?? 0, parts.minute ?? 0, parts.second ?? 0);
  return local - Math.floor(unixMs / 1000) * 1000;
}

/** When a day starts in `timeZone`, in Unix ms. */
export function dayStart(day: string, timeZone?: string): number {
  const [y, m, d] = day.split("-").map(Number);
  const utc = Date.UTC(y ?? 1970, (m ?? 1) - 1, d ?? 1);
  // Twice: the offset at midnight may differ from the offset at the guess.
  const guess = utc - offset(utc, timeZone);
  return utc - offset(guess, timeZone);
}

/** A ratio as a percentage, `12.5 %`. */
export function percent(ratio: number | null | undefined, language?: string): string {
  if (ratio === null || ratio === undefined || !Number.isFinite(ratio)) {
    return "–";
  }
  return new Intl.NumberFormat(language, { style: "percent", maximumFractionDigits: 1 }).format(ratio);
}
