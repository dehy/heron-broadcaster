// SPDX-License-Identifier: AGPL-3.0-or-later

import { useRef } from "react";

/**
 * Keys for the rows of a list edited in place, whose items have no ID: a
 * key per row, kept while the row is edited and dropped with it, so that
 * React keeps each row's elements (and the focus) where they are. Rows
 * added at the end get new keys; call `remove` with the index of a row
 * taken out.
 */
export function useRowKeys(count: number): { keys: readonly number[]; remove: (index: number) => void } {
  const state = useRef({ keys: [] as number[], next: 0 });
  const current = state.current;
  while (current.keys.length < count) current.keys.push(current.next++);
  current.keys.length = Math.min(current.keys.length, count);
  return {
    keys: current.keys,
    remove: (index) => {
      current.keys.splice(index, 1);
    },
  };
}
