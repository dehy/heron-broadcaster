// SPDX-License-Identifier: AGPL-3.0-or-later

import { describe, expect, it } from "vitest";
import { withNextStart } from "./nextStart";

const NONE = { crossfade: null, fade_in: null, fade_out: null, fixed_ms: null, mode: null } as const;

describe("where the next track starts", () => {
  it("makes a fixed crossfade that long before the cue-out", () => {
    const next = withNextStart({ cue_in_ms: null, cue_out_ms: null, gap_killer_enabled: null, crossfade: { ...NONE } }, 170_040, 178_000);
    expect(next.crossfade).toEqual({ ...NONE, mode: "fixed", fixed_ms: 8_000 });
  });

  it("turns crossfading back on, and stops at the cue-out", () => {
    const next = withNextStart({ cue_in_ms: null, cue_out_ms: 178_000, gap_killer_enabled: null, crossfade: { ...NONE, crossfade: false } }, 190_000, 178_000);
    expect(next.cue_out_ms).toBe(178_000);
    expect(next.crossfade?.crossfade).toBeNull();
    expect(next.crossfade?.fixed_ms).toBe(0);
  });
});
