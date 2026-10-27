// SPDX-License-Identifier: AGPL-3.0-or-later

import { describe, expect, it } from "vitest";
import { introFromCueIn, jingleStart } from "./intro";

const song = { cue_in_ms: 1_000, cue_out_ms: 200_000, intro_ms: 21_000 };
const jingle = { cue_in_ms: 100, cue_out_ms: 6_100 };

describe("intros", () => {
  it("counts the intro from the cue-in", () => {
    expect(introFromCueIn(song)).toBe(20_000);
    expect(introFromCueIn({ ...song, intro_ms: null })).toBeNull();
    // Before the cue-in, or after the cue-out: unusable.
    expect(introFromCueIn({ ...song, intro_ms: 500 })).toBeNull();
    expect(introFromCueIn({ ...song, intro_ms: 300_000 })).toBeNull();
  });

  it("ends a jingle on the intro, when it fits", () => {
    expect(jingleStart(song, jingle)).toBe(14_000);
    expect(jingleStart({ ...song, intro_ms: 5_000 }, jingle)).toBeNull();
    expect(jingleStart({ ...song, intro_ms: null }, jingle)).toBeNull();
  });
});
