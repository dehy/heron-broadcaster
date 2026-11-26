// @vitest-environment jsdom
// SPDX-License-Identifier: AGPL-3.0-or-later

import { describe, expect, it } from "vitest";
import { fromName } from "./BufferReview";

describe("fromName", () => {
  it("reads an artist, a title and a track number", () => {
    expect(fromName("a/Björk - Hunter.mp3")).toEqual({ artist: "Björk", title: "Hunter" });
    expect(fromName("01 - Björk - Hunter.flac")).toEqual({ artist: "Björk", title: "Hunter", track: "1" });
    expect(fromName("07. Hunter.flac")).toEqual({ title: "Hunter", track: "7" });
    expect(fromName("Hunter.mp3")).toBeNull();
  });
});
