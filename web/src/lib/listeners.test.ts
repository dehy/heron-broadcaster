// SPDX-License-Identifier: AGPL-3.0-or-later

import { describe, expect, it } from "vitest";
import { addDays, dayOf, dayStart, percent } from "./listeners";

describe("days in station time", () => {
  it("writes the day of a time in a zone", () => {
    const at = Date.UTC(2026, 9, 25, 22, 30);
    expect(dayOf(at, "UTC")).toBe("2026-10-25");
    expect(dayOf(at, "Europe/Paris")).toBe("2026-10-25");
    expect(dayOf(at, "Asia/Tokyo")).toBe("2026-10-26");
  });

  it("adds days across months", () => {
    expect(addDays("2026-10-31", 1)).toBe("2026-11-01");
    expect(addDays("2026-03-01", -1)).toBe("2026-02-28");
  });

  it("finds midnight, also on days the clocks change", () => {
    expect(dayStart("2026-10-25", "UTC")).toBe(Date.UTC(2026, 9, 25));
    // Summer time until 03:00: midnight is 22:00 UTC the day before.
    expect(dayStart("2026-10-25", "Europe/Paris")).toBe(Date.UTC(2026, 9, 24, 22));
    expect(dayStart("2026-10-26", "Europe/Paris")).toBe(Date.UTC(2026, 9, 25, 23));
    expect(dayStart("2026-10-26", "America/New_York")).toBe(Date.UTC(2026, 9, 26, 4));
  });

  it("writes ratios as percentages", () => {
    expect(percent(0.125, "en")).toBe("12.5%");
    expect(percent(null, "en")).toBe("–");
  });
});
