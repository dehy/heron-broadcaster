// SPDX-License-Identifier: AGPL-3.0-or-later

import { describe, expect, it } from "vitest";
import {
  type Grid,
  cell,
  civilLabel,
  copyDay,
  everyDay,
  joinCivil,
  paint,
  periodProblem,
  rectangle,
  splitCivil,
  wholeDays,
} from "./schedule";

describe("painting the week", () => {
  it("covers a day, an hour of every day, a rectangle", () => {
    expect(wholeDays([1, 2])).toHaveLength(48);
    expect(everyDay(7).map((h) => h.day)).toEqual([1, 2, 3, 4, 5, 6, 7]);
    // Friday 10:00 back to Monday 06:00: Monday to Friday, 06:00 to 10:00.
    const rect = rectangle({ day: 5, hour: 10 }, { day: 1, hour: 6 });
    expect(rect).toHaveLength(5 * 5);
    expect(rect[0]).toEqual({ day: 1, hour: 6 });
  });

  it("paints, clears, and keeps the hours that have a clock when asked", () => {
    const grid: Grid = new Map([[cell(1, 7), 9]]);
    const all = paint(grid, wholeDays([1]), 3);
    expect(all.get(cell(1, 7))).toBe(3);
    expect(all.size).toBe(24);
    const empty = paint(grid, wholeDays([1]), 3, true);
    expect(empty.get(cell(1, 7))).toBe(9);
    expect(empty.get(cell(1, 8))).toBe(3);
    expect(paint(all, everyDay(7), null).has(cell(1, 7))).toBe(false);
    expect(grid.size).toBe(1);
  });

  it("copies a day, its empty hours too", () => {
    const grid: Grid = new Map([
      [cell(1, 6), 4],
      [cell(2, 9), 5],
    ]);
    const copied = copyDay(grid, 1, [1, 2, 3]);
    expect(copied.get(cell(2, 6))).toBe(4);
    expect(copied.has(cell(2, 9))).toBe(false);
    expect(copied.get(cell(3, 6))).toBe(4);
    expect(copied.get(cell(1, 6))).toBe(4);
  });
});

describe("periods", () => {
  it("splits and joins a station time", () => {
    expect(splitCivil("2026-12-01T06:00:00")).toEqual({ date: "2026-12-01", hour: 6 });
    expect(joinCivil({ date: "2026-12-31", hour: 20 })).toBe("2026-12-31T20:00:00");
  });

  it("writes a period's bounds, without the year every year", () => {
    expect(civilLabel("2026-12-01T00:00:00", false, "en-GB")).toBe("1 December 2026 at 00:00");
    expect(civilLabel("2026-12-01T00:00:00", true, "en-GB")).toBe("1 December at 00:00");
  });

  it("finds the periods the API refuses", () => {
    expect(periodProblem({ start: "2026-12-01T00:00:00", end: "2027-01-01T00:00:00" })).toBeNull();
    expect(periodProblem({ start: "2027-01-01T00:00:00", end: "2026-12-01T00:00:00" })).toBe("backwards");
    expect(periodProblem({ start: "2026-12-20T00:00:00", end: "2026-01-06T00:00:00", every_year: true })).toBeNull();
    expect(periodProblem({ start: "2026-12-01T00:00:00", end: "2030-12-01T00:00:00", every_year: true })).toBe("empty");
  });
});
