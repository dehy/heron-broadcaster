// @vitest-environment jsdom
// SPDX-License-Identifier: AGPL-3.0-or-later

import { renderHook } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { useRowKeys } from "./rowKeys";

describe("useRowKeys", () => {
  it("keeps each row's key while rows are added and removed", () => {
    const { result, rerender } = renderHook(({ count }) => useRowKeys(count), { initialProps: { count: 3 } });
    const [a, b, c] = result.current.keys;
    expect(new Set([a, b, c]).size).toBe(3);
    result.current.remove(1);
    rerender({ count: 2 });
    expect(result.current.keys).toEqual([a, c]);
    rerender({ count: 3 });
    const added = result.current.keys[2];
    expect([a, b, c]).not.toContain(added);
    expect(result.current.keys.slice(0, 2)).toEqual([a, c]);
  });

  it("follows a list that shrinks or grows on its own", () => {
    const { result, rerender } = renderHook(({ count }) => useRowKeys(count), { initialProps: { count: 2 } });
    const first = result.current.keys[0];
    rerender({ count: 1 });
    expect(result.current.keys).toEqual([first]);
    rerender({ count: 0 });
    expect(result.current.keys).toEqual([]);
  });
});
