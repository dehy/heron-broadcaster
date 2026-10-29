// SPDX-License-Identifier: AGPL-3.0-or-later

import { describe, expect, it } from "vitest";
import { layout } from "../lib/medley";
import { junctions } from "./MedleyTimeline";

describe("junctions", () => {
  const recipe = { crossfade_ms: 200, liner_mode: "over" };
  const parts = [
    { kind: "hook", from_ms: 10_000, to_ms: 18_000 },
    { kind: "liner", from_ms: 0, to_ms: 2_000 },
    { kind: "hook", from_ms: 30_000, to_ms: 36_000 },
  ];

  it("shares a liner half and half by default", () => {
    const [join] = junctions(parts, layout(parts, recipe).parts, 200);
    expect(join?.liner).toBe(1);
    expect(join?.share).toBeCloseTo(0.5, 2);
  });

  it("follows the shift, and leaves out liners at the edges", () => {
    const shifted = parts.map((part, i) => (i === 1 ? { ...part, shift_ms: 500 } : part));
    const [join] = junctions(shifted, layout(shifted, recipe).parts, 200);
    expect(join?.share).toBeCloseTo(0.75, 2);
    const edges = [{ kind: "liner", from_ms: 0, to_ms: 1_000 }, ...parts.slice(0, 1)];
    expect(junctions(edges, layout(edges, recipe).parts, 200)).toEqual([]);
  });
});
