// SPDX-License-Identifier: AGPL-3.0-or-later

// A medley as it will sound (docs/medleys.md): the hooks with their
// crossfades and the extensions under liners over the music, the liners,
// and the fade-out. Under each liner over the music, the point where the
// music passes from one hook to the next can be dragged to share the
// liner between them.

import { type KeyboardEvent, type PointerEvent, useRef } from "react";
import { t } from "../i18n";
import { duration, trackName } from "../lib/format";
import { type LayoutPart, type Placed, layout } from "../lib/medley";

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

export function MedleyTimeline({
  parts,
  recipe,
  onShift,
}: {
  parts: readonly TimelinePart[];
  recipe: { crossfade_ms: number; liner_mode: string; fade_out_ms: number };
  /** Moves the liner at `index` to `shift_ms`; none: read only. */
  onShift?: ((index: number, shift_ms: number) => void) | undefined;
}) {
  const box = useRef<HTMLDivElement>(null);
  const drag = useRef<{ index: number; x: number; shift: number } | null>(null);
  const { parts: placed, length_ms, music_end_ms } = layout(parts, recipe);
  if (length_ms === 0) {
    return null;
  }
  const pct = (ms: number) => `${(ms / length_ms) * 100}%`;
  const width = (from: number, to: number) => `${(Math.max(0, to - from) / length_ms) * 100}%`;
  const joins = junctions(parts, placed, recipe.crossfade_ms);
  const fade = Math.min(recipe.fade_out_ms, music_end_ms);
  let hookLane = 0;

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
      <div className="lanes">
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
              className={`block ${item.kind}${item.over ? " over" : ""}`}
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
