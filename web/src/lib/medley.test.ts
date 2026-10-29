// SPDX-License-Identifier: AGPL-3.0-or-later

import { describe, expect, it } from "vitest";
import { displayName, layout, linerSlot, medleyName, plannedLength, renderMonth } from "./medley";

const parts = [
  { kind: "hook", from_ms: 10_000, to_ms: 17_000 },
  { kind: "liner", from_ms: 0, to_ms: 3_000 },
  { kind: "hook", from_ms: 5_000, to_ms: 12_000 },
];

describe("medleyName", () => {
  it("lists each artist once, in order", () => {
    expect(medleyName("2026-10", ["Helena", "Panic! At The Disco", "Mabel"])).toBe(
      "Medley 2026-10 (Helena - Panic! At The Disco - Mabel)",
    );
    expect(medleyName("2026-10", ["Mabel", null, " ", "mabel", "Helena"])).toBe("Medley 2026-10 (Mabel - Helena)");
    expect(medleyName("2026-11", [])).toBe("Medley 2026-11");
  });

  it("cuts a long list after the last name that fits", () => {
    const names = Array.from({ length: 30 }, (_, i) => `Artist number ${String(i + 1).padStart(2, "0")}`);
    const name = medleyName("2026-10", names);
    expect([...name].length).toBeLessThanOrEqual(249);
    expect(name.endsWith(" - …)")).toBe(true);
    const cut = medleyName("2026-10", ["x".repeat(400)]);
    expect([...cut].length).toBe(249);
    expect(cut.endsWith("x…)")).toBe(true);
  });
});

describe("renderMonth", () => {
  it("is the year and month in the station's zone", () => {
    // 23:30 UTC on 31 October is already November in Paris.
    const date = new Date(Date.UTC(2026, 9, 31, 23, 30));
    expect(renderMonth(date, "UTC")).toBe("2026-10");
    expect(renderMonth(date, "Europe/Paris")).toBe("2026-11");
  });
});

describe("displayName", () => {
  it("stands in until the first render", () => {
    expect(displayName({ id: 4, name: "Medley 2026-10 (A)" })).toBe("Medley 2026-10 (A)");
    expect(displayName({ id: 4, name: null })).toContain("4");
  });
});

describe("linerSlot", () => {
  const plan = [{ kind: "liner" }, { kind: "hook" }, { kind: "liner" }, { kind: "hook" }, { kind: "liner" }];
  it("is the start before every hook, the end after every hook", () => {
    expect(linerSlot(plan, 0)).toBe("start");
    expect(linerSlot(plan, 2)).toBe("between");
    expect(linerSlot(plan, 4)).toBe("end");
    expect(linerSlot(plan, plan.length)).toBe("end");
  });
});

describe("plannedLength", () => {
  it("overlaps parts by the crossfade", () => {
    expect(plannedLength(parts, { crossfade_ms: 500, liner_mode: "between" })).toBe(17_000 - 2 * 500);
    expect(plannedLength([], { crossfade_ms: 500, liner_mode: "over" })).toBe(0);
  });
});

// The same cases as heron_core::medley's tests: the two layouts must agree.
describe("layout", () => {
  const over = { crossfade_ms: 200, liner_mode: "over" };
  const hooks = [
    { kind: "hook", from_ms: 10_000, to_ms: 18_000 },
    { kind: "liner", from_ms: 0, to_ms: 2_000 },
    { kind: "hook", from_ms: 30_000, to_ms: 36_000 },
  ];

  it("puts liners over the music between whole hooks, half each", () => {
    const { parts: [first, liner, second] = [], length_ms } = layout(hooks, over);
    expect([first?.from_ms, first?.to_ms]).toEqual([10_000, 19_250]);
    expect([second?.from_ms, second?.to_ms, second?.at_ms]).toEqual([28_750, 36_000, 9_050]);
    expect(liner?.at_ms).toBe(8_150);
    expect(length_ms).toBe(9_050 + 7_250);
  });

  it("shares the gap as the liner says, within the files", () => {
    const shifted = hooks.map((part, i) => (i === 1 ? { ...part, shift_ms: 500 } : part));
    const moved = layout(shifted, over).parts;
    expect([moved[0]?.to_ms, moved[2]?.from_ms, moved[1]?.at_ms]).toEqual([19_750, 29_250, 8_150]);
    const short = hooks.map((part, i) => (i === 0 ? { ...part, duration_ms: 18_500 } : part));
    const cut = layout(short, over).parts;
    expect([cut[0]?.to_ms, cut[2]?.from_ms]).toEqual([18_500, 28_000]);
  });

  it("extends one hook for a liner at the start or the end", () => {
    const { parts: [opening, hook, closing] = [] } = layout(
      [
        { kind: "liner", from_ms: 0, to_ms: 2_000 },
        { kind: "hook", from_ms: 5_000, to_ms: 9_000 },
        { kind: "liner", from_ms: 0, to_ms: 1_000 },
      ],
      over,
    );
    expect([hook?.at_ms, hook?.from_ms, hook?.to_ms]).toEqual([0, 2_850, 10_150]);
    expect(opening?.at_ms).toBe(0);
    expect(closing?.at_ms).toBe(9_000 - 2_850 + 150);
  });
});
