// SPDX-License-Identifier: AGPL-3.0-or-later

import { describe, expect, it } from "vitest";
import { clickRow } from "./rangeSelection";

const rows = [8, 9, 10, 11, 12];
const sorted = (set: Set<number>) => [...set].sort((a, b) => a - b);

describe("clickRow", () => {
  it("toggles one row on a plain click", () => {
    expect([...clickRow(rows, new Set(), null, 9, false)]).toEqual([9]);
    expect([...clickRow(rows, new Set([9]), 9, 9, false)]).toEqual([]);
  });

  it("gives the range the clicked checkbox's new state", () => {
    // Check 8, then Shift + click 12: 8 to 12.
    expect(sorted(clickRow(rows, new Set([8]), 8, 12, true))).toEqual(rows);
    // Then uncheck 9 to 11, from 9 (now unchecked) as the anchor.
    const all = new Set(rows);
    all.delete(9);
    expect(sorted(clickRow(rows, all, 9, 11, true))).toEqual([8, 12]);
    // Upwards too.
    expect(sorted(clickRow(rows, new Set(), 11, 9, true))).toEqual([9, 10, 11]);
  });

  it("toggles one row when the anchor is not shown", () => {
    expect([...clickRow(rows, new Set(), 3, 10, true)]).toEqual([10]);
  });
});
