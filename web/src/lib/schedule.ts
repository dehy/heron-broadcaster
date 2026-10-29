// SPDX-License-Identifier: AGPL-3.0-or-later

// Painting the week with clocks (docs/web.md › Painting the schedule):
// which hours a stroke covers, and what the week looks like after it.

/** Clock IDs by `day-hour` (day 1 = Monday … 7, hour 0 to 23). */
export type Grid = Map<string, number>;

export interface Hour {
  day: number;
  hour: number;
}

export const DAYS = [1, 2, 3, 4, 5, 6, 7] as const;
export const HOURS = Array.from({ length: 24 }, (_, hour) => hour);
export const WEEKDAYS = [1, 2, 3, 4, 5] as const;
export const WEEKEND = [6, 7] as const;

export const cell = (day: number, hour: number) => `${day}-${hour}`;

/** Every hour of these days. */
export function wholeDays(days: readonly number[]): Hour[] {
  return days.flatMap((day) => HOURS.map((hour) => ({ day, hour })));
}

/** This hour on every day. */
export function everyDay(hour: number): Hour[] {
  return DAYS.map((day) => ({ day, hour }));
}

/** Every hour of the rectangle between two hours, both included. */
export function rectangle(a: Hour, b: Hour): Hour[] {
  const hours: Hour[] = [];
  for (let day = Math.min(a.day, b.day); day <= Math.max(a.day, b.day); day += 1) {
    for (let hour = Math.min(a.hour, b.hour); hour <= Math.max(a.hour, b.hour); hour += 1) {
      hours.push({ day, hour });
    }
  }
  return hours;
}

/**
 * The week with `hours` painted with `clock` (none clears them); with
 * `onlyEmpty`, the hours that already have a clock keep it.
 */
export function paint(grid: Grid, hours: readonly Hour[], clock: number | null, onlyEmpty = false): Grid {
  const next = new Map(grid);
  for (const { day, hour } of hours) {
    const key = cell(day, hour);
    if (onlyEmpty && grid.has(key)) {
      continue;
    }
    if (clock === null) {
      next.delete(key);
    } else {
      next.set(key, clock);
    }
  }
  return next;
}

/** The week with the hours of day `from` copied onto each of `to`, empty hours included. */
export function copyDay(grid: Grid, from: number, to: readonly number[]): Grid {
  const next = new Map(grid);
  for (const day of to) {
    if (day === from) {
      continue;
    }
    for (const hour of HOURS) {
      const clock = grid.get(cell(from, hour));
      if (clock === undefined) {
        next.delete(cell(day, hour));
      } else {
        next.set(cell(day, hour), clock);
      }
    }
  }
  return next;
}
