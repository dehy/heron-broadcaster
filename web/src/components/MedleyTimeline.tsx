// SPDX-License-Identifier: AGPL-3.0-or-later

// A medley as it will sound (docs/medleys.md): the hooks with their
// crossfades and the extensions under liners over the music, the liners,
// and the fade-out. Under each liner over the music, the point where the
// music passes from one hook to the next can be dragged to share the
// liner between them. Every part can be dragged to move it in the medley;
// a click without moving plays from that point.

import { type KeyboardEvent, type PointerEvent, useRef } from "react";
import { t } from "../i18n";
import { duration, trackName } from "../lib/format";
import { type LayoutPart, type Placed, layout, moveTo } from "../lib/medley";

export interface TimelinePart extends LayoutPart {
  title?: string | null;
  artist?: string | null;
  path: string;
}

/** One step of the keyboard and of the − / + buttons, in ms. */
export const SHIFT_STEP_MS = 100;

/**
 * Under each group of liners over the music between two hooks: where the
 * music passes from one to the other, and how much of the liners is over
 * the hook before (0 to 1).
 */
export function junctions(
  parts: readonly LayoutPart[],
  placed: readonly Placed[],
  crossfade_ms: number,
): { liner: number; at_ms: number; share: number }[] {
  const found: { liner: number; at_ms: number; share: number }[] = [];
  parts.forEach((_, index) => {
    const previous = parts[index - 1];
    if (!placed[index]?.over || previous === undefined || previous.kind !== "hook") {
      return;
    }
    // The first liner of its group, between two hooks.
    let last = index;
    while (placed[last + 1]?.over) {
      last += 1;
    }
    const next = placed[last + 1];
    const first = placed[index];
    const end = placed[last];
    if (!next || !first || !end) {
      return;
    }
    const at_ms = next.at_ms + crossfade_ms / 2;
    const span = end.at_ms + (end.to_ms - end.from_ms) - first.at_ms;
    const share = span > 0 ? Math.min(1, Math.max(0, (at_ms - first.at_ms) / span)) : 0.5;
    found.push({ liner: index, at_ms, share });
  });
  return found;
}

/** How far the pointer moves before a press is a drag, not a click. */
const DRAG_PX = 4;

export function MedleyTimeline({
  parts,
  recipe,
  rendered,
  playhead_ms,
  onShift,
  onMove,
  onSeek,
}: {
  parts: readonly TimelinePart[];
  recipe: { crossfade_ms: number; liner_mode: string; fade_out_ms: number };
  /** The layout the medley's file was rendered with: drawn instead of laying the parts out again. */
  rendered?: { parts: Placed[]; length_ms: number; music_end_ms: number; fade_out_ms?: number } | null | undefined;
  /** Where the player is on the timeline; none when it does not play. */
  playhead_ms?: number | null | undefined;
  /** Moves the liner at `index` to `shift_ms`; none: read only. */
  onShift?: ((index: number, shift_ms: number) => void) | undefined;
  /** Moves the part at `index` to `move_ms`; none: read only. */
  onMove?: ((index: number, move_ms: number) => void) | undefined;
  /** Plays from `at_ms` on the timeline. */
  onSeek?: ((at_ms: number) => void) | undefined;
}) {
  const box = useRef<HTMLDivElement>(null);
  const drag = useRef<{ index: number; x: number; shift: number } | null>(null);
  // A press on the lanes: a click (seek) or, once it moves, a drag of a part.
  const press = useRef<{ index: number | null; x: number; at: number; dragging: boolean } | null>(null);
  const { parts: placed, length_ms, music_end_ms } = rendered ?? layout(parts, recipe);
  if (length_ms === 0) {
    return null;
  }
  const pct = (ms: number) => `${(ms / length_ms) * 100}%`;
  const width = (from: number, to: number) => `${(Math.max(0, to - from) / length_ms) * 100}%`;
  const joins = junctions(parts, placed, recipe.crossfade_ms);
  const fade = Math.min(rendered ? (rendered.fade_out_ms ?? 0) : recipe.fade_out_ms, music_end_ms);
  let hookLane = 0;

  const msAt = (clientX: number) => {
    const rect = box.current?.querySelector(".lanes")?.getBoundingClientRect();
    return rect && rect.width > 0 ? Math.min(length_ms, Math.max(0, ((clientX - rect.left) / rect.width) * length_ms)) : 0;
  };
  const msPerPx = () => {
    const rect = box.current?.querySelector(".lanes")?.getBoundingClientRect();
    return rect && rect.width > 0 ? length_ms / rect.width : 0;
  };
  const pressDown = (event: PointerEvent<HTMLDivElement>) => {
    if (event.button !== 0 || (event.target as HTMLElement).closest(".junction")) {
      return;
    }
    const block = (event.target as HTMLElement).closest<HTMLElement>("[data-index]");
    const index = block ? Number(block.dataset.index) : null;
    event.currentTarget.setPointerCapture(event.pointerId);
    press.current = { index, x: event.clientX, at: index !== null ? (placed[index]?.at_ms ?? 0) : 0, dragging: false };
  };
  const pressMove = (event: PointerEvent<HTMLDivElement>) => {
    const current = press.current;
    if (!current || current.index === null || !onMove) {
      return;
    }
    const dx = event.clientX - current.x;
    if (!current.dragging && Math.abs(dx) < DRAG_PX) {
      return;
    }
    current.dragging = true;
    const at = current.at + dx * msPerPx();
    onMove(current.index, Math.round(moveTo(parts, recipe, current.index, at) / 10) * 10);
  };
  const pressUp = (event: PointerEvent<HTMLDivElement>) => {
    const current = press.current;
    press.current = null;
    if (current && !current.dragging && onSeek) {
      onSeek(msAt(event.clientX));
    }
  };

  const shiftOf = (index: number) => parts[index]?.shift_ms ?? 0;
  const startDrag = (index: number) => (event: PointerEvent<HTMLButtonElement>) => {
    if (!onShift) {
      return;
    }
    event.currentTarget.setPointerCapture(event.pointerId);
    drag.current = { index, x: event.clientX, shift: shiftOf(index) };
  };
  const moveDrag = (event: PointerEvent<HTMLButtonElement>) => {
    const current = drag.current;
    const rect = box.current?.getBoundingClientRect();
    if (!current || !rect || rect.width === 0 || !onShift) {
      return;
    }
    const ms = ((event.clientX - current.x) / rect.width) * length_ms;
    onShift(current.index, Math.round((current.shift + ms) / 10) * 10);
  };
  const endDrag = () => {
    drag.current = null;
  };
  const key = (index: number) => (event: KeyboardEvent<HTMLButtonElement>) => {
    if (!onShift) {
      return;
    }
    const by = event.key === "ArrowRight" ? SHIFT_STEP_MS : event.key === "ArrowLeft" ? -SHIFT_STEP_MS : 0;
    if (by !== 0) {
      event.preventDefault();
      onShift(index, shiftOf(index) + by);
    } else if (event.key === "Home") {
      event.preventDefault();
      onShift(index, 0);
    }
  };

  return (
    <div className="medley-timeline" ref={box} role="group" aria-label={t("medleys.timeline")}>
      <div
        className={`lanes${onMove ? " movable" : ""}${onSeek ? " seekable" : ""}`}
        onPointerDown={pressDown}
        onPointerMove={pressMove}
        onPointerUp={pressUp}
        onPointerCancel={() => {
          press.current = null;
        }}
      >
        {placed.map((item, index) => {
          const part = parts[index];
          if (!part) {
            return null;
          }
          const lane = item.kind === "hook" ? 1 + (hookLane++ % 2) : 0;
          const name = trackName(part);
          // The extensions: before and after the span chosen to be heard.
          const before = part.from_ms - item.from_ms;
          const after = item.to_ms - part.to_ms;
          const span = item.to_ms - item.from_ms;
          return (
            <div
              key={`${index}-${part.from_ms}`}
              data-index={index}
              className={`block ${item.kind}${item.over ? " over" : ""}${part.move_ms ? " moved" : ""}`}
              style={{ left: pct(item.at_ms), width: pct(span), top: `${lane * 1.9}rem` }}
              title={`${name} · ${duration(item.at_ms, true)} – ${duration(item.at_ms + span, true)}`}
            >
              {before > 0 && <span className="extension" style={{ left: 0, width: `${(before / span) * 100}%` }} />}
              {after > 0 && <span className="extension" style={{ right: 0, width: `${(after / span) * 100}%` }} />}
              <span className="label">{name}</span>
            </div>
          );
        })}
        {fade > 0 && (
          <div
            className="fade-out"
            style={{ left: pct(music_end_ms - fade), width: width(music_end_ms - fade, music_end_ms) }}
            title={t("medleys.fadeOut")}
          />
        )}
        {playhead_ms != null && <div className="playhead" style={{ left: pct(playhead_ms) }} />}
        {joins.map((join) => (
          <button
            key={join.liner}
            type="button"
            className="junction"
            role="slider"
            aria-label={t("medleys.junction", { name: trackName(parts[join.liner] ?? { path: "" }) })}
            aria-valuemin={0}
            aria-valuemax={100}
            aria-valuenow={Math.round(join.share * 100)}
            aria-valuetext={t("medleys.share", { before: String(Math.round(join.share * 100)) })}
            aria-readonly={!onShift}
            disabled={!onShift}
            style={{ left: pct(join.at_ms) }}
            onPointerDown={startDrag(join.liner)}
            onPointerMove={moveDrag}
            onPointerUp={endDrag}
            onPointerCancel={endDrag}
            onKeyDown={key(join.liner)}
          >
            <span className="share">{Math.round(join.share * 100)} %</span>
          </button>
        ))}
      </div>
      <div className="axis small muted">
        <span>0:00</span>
        <span>{duration(length_ms, true)}</span>
      </div>
    </div>
  );
}
