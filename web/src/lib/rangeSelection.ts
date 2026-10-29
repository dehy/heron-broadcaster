// SPDX-License-Identifier: AGPL-3.0-or-later

// Selecting rows of a list (docs/web.md › Selecting rows): a click on a
// checkbox toggles its row and makes it the anchor; Shift + click gives
// every row from the anchor to this one the clicked checkbox's new state;
// Ctrl/⌘ + click anywhere on a row toggles it without opening it.

import { type MouseEvent, useEffect, useRef } from "react";

/**
 * The selection after a click on the checkbox of `id`: with `shift` and an
 * anchor among `order` (the rows as shown), the whole range between them
 * takes the new state of `id`.
 */
export function clickRow<Id>(
  order: readonly Id[],
  selected: ReadonlySet<Id>,
  anchor: Id | null,
  id: Id,
  shift: boolean,
): Set<Id> {
  const next = new Set(selected);
  const on = !selected.has(id);
  const from = anchor === null ? -1 : order.indexOf(anchor);
  const to = order.indexOf(id);
  const range =
    shift && from >= 0 && to >= 0 ? order.slice(Math.min(from, to), Math.max(from, to) + 1) : [id];
  for (const row of range) {
    if (on) {
      next.add(row);
    } else {
      next.delete(row);
    }
  }
  return next;
}

export interface RangeSelection<Id> {
  /** For the row's checkbox: `onClick`, with a no-op `onChange`. */
  checkbox: (id: Id) => {
    onClick: (event: MouseEvent<HTMLInputElement>) => void;
    onChange: () => void;
  };
  /** For the row (`onClickCapture`): Ctrl/⌘ + click toggles it instead of opening it. */
  row: (id: Id) => (event: MouseEvent<HTMLElement>) => void;
}

/**
 * Range selection over `order`, the rows as shown. `reset` changes when
 * the rows shown do (another page, other filters): the anchor is dropped.
 */
export function useRangeSelection<Id>(
  order: readonly Id[],
  selected: ReadonlySet<Id>,
  setSelected: (next: Set<Id>) => void,
  reset: unknown,
): RangeSelection<Id> {
  const anchor = useRef<Id | null>(null);
  useEffect(() => {
    anchor.current = null;
  }, [reset]);
  const click = (id: Id, shift: boolean) => {
    setSelected(clickRow(order, selected, anchor.current, id, shift));
    // Shift + click keeps the anchor, so another one changes the range.
    if (!shift || anchor.current === null) {
      anchor.current = id;
    }
  };
  return {
    checkbox: (id) => ({
      onClick: (event) => click(id, event.shiftKey),
      onChange: () => {},
    }),
    row: (id) => (event) => {
      if (!(event.ctrlKey || event.metaKey) || (event.target as HTMLElement).closest("input[type=checkbox]")) {
        return;
      }
      event.preventDefault();
      event.stopPropagation();
      click(id, false);
    },
  };
}
