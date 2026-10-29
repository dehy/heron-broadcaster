// SPDX-License-Identifier: AGPL-3.0-or-later

import { describe, expect, it } from "vitest";
import { displayName, linerSlot, medleyName, plannedLength, renderMonth } from "./medley";

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
  });

  it("gives liners over the music no time of their own", () => {
    expect(plannedLength(parts, { crossfade_ms: 500, liner_mode: "over" })).toBe(14_000 - 500);
    expect(plannedLength([], { crossfade_ms: 500, liner_mode: "over" })).toBe(0);
  });
});
