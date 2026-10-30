// SPDX-License-Identifier: AGPL-3.0-or-later

import { describe, expect, it } from "vitest";
import { COLORS, nextColor } from "./colors";

describe("nextColor", () => {
  it("takes the first color no category has", () => {
    expect(nextColor([])).toBe(COLORS[0]);
    expect(nextColor([COLORS[0]?.toUpperCase(), null])).toBe(COLORS[1]);
  });

  it("goes round once every color is taken", () => {
    expect(nextColor([...COLORS, COLORS[0]])).toBe(COLORS[1]);
  });
});
