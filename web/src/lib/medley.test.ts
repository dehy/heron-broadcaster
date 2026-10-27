// SPDX-License-Identifier: AGPL-3.0-or-later

import { describe, expect, it } from "vitest";
import { copyName, plannedLength } from "./medley";

const parts = [
  { kind: "hook", from_ms: 10_000, to_ms: 17_000 },
  { kind: "liner", from_ms: 0, to_ms: 3_000 },
  { kind: "hook", from_ms: 5_000, to_ms: 12_000 },
];

describe("copyName", () => {
  it("adds (copy), then a number when taken", () => {
    expect(copyName("Hits", [])).toBe("Hits (copy)");
    expect(copyName("Hits", ["Hits", "Hits (copy)", "Hits (copy 2)"])).toBe("Hits (copy 3)");
  });
});

describe("plannedLength", () => {
  it("overlaps parts by the crossfade", () => {
    expect(plannedLength(parts, { crossfade_ms: 500, liner_mode: "between" })).toBe(17_000 - 2 * 500);
  });

  it("gives liners over the music no time of their own", () => {
    expect(plannedLength(parts, { crossfade_ms: 500, liner_mode: "over" })).toBe(14_000 - 500);
    expect(plannedLength([], { crossfade_ms: 500, liner_mode: "over" })).toBe(0);
  });
});
