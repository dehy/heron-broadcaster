// SPDX-License-Identifier: AGPL-3.0-or-later

// Medleys (docs/medleys.md): what the web shows before a render.

import { t } from "../i18n";
import { timeZone } from "./format";

/** The longest medley name, as the API cuts it (heron_core::medley::NAME_CHARS). */
const NAME_CHARS = 249;

/**
 * The name a medley gets at its render, as the API gives it: `Medley
 * 2026-10 (A - B - C)`, each artist (or title) once, the list cut with `…`
 * when too long. The API may add ` (2)` when another medley has it.
 */
export function medleyName(month: string, names: readonly (string | null | undefined)[]): string {
  const base = `Medley ${month}`;
  const seen = new Set<string>();
  const unique: string[] = [];
  for (const raw of names) {
    const name = (raw ?? "").trim();
    if (name && !seen.has(name.toLowerCase())) {
      seen.add(name.toLowerCase());
      unique.push(name);
    }
  }
  if (unique.length === 0) {
    return base;
  }
  const length = (text: string) => [...text].length;
  for (let count = unique.length; count > 0; count -= 1) {
    const list = unique.slice(0, count).join(" - ") + (count < unique.length ? " - …" : "");
    const name = `${base} (${list})`;
    if (length(name) <= NAME_CHARS) {
      return name;
    }
  }
  const room = NAME_CHARS - length(base) - length(" (…)");
  return `${base} (${[...(unique[0] ?? "")].slice(0, room).join("")}…)`;
}

/** Year and month of a date in the station's time zone: `2026-10`. */
export function renderMonth(date: Date, zone: string | undefined = timeZone()): string {
  const parts = new Intl.DateTimeFormat("en", { year: "numeric", month: "2-digit", timeZone: zone }).formatToParts(date);
  const part = (type: string) => parts.find((p) => p.type === type)?.value ?? "";
  return `${part("year")}-${part("month")}`;
}

/** A medley's name; a stand-in until its first render names it. */
export function displayName(medley: { id: number; name?: string | null }): string {
  return medley.name ?? t("medleys.unnamed", { id: String(medley.id) });
}

/** Where the liner at `index` goes: before every hook, after every hook, or between. */
export function linerSlot(parts: readonly { kind: string }[], index: number): "start" | "between" | "end" {
  if (!parts.slice(0, index).some((part) => part.kind === "hook")) {
    return "start";
  }
  if (!parts.slice(index + 1).some((part) => part.kind === "hook")) {
    return "end";
  }
  return "between";
}

interface Span {
  kind: string;
  from_ms: number;
  to_ms: number;
}

/**
 * How long the parts last together once mixed: consecutive parts overlap
 * by the crossfade; liners over the music take no time of their own.
 */
export function plannedLength(parts: Span[], recipe: { crossfade_ms: number; liner_mode: string }): number {
  const over = recipe.liner_mode === "over";
  const sequence = parts.filter((part) => !(over && part.kind === "liner"));
  const total = sequence.reduce((sum, part) => sum + (part.to_ms - part.from_ms), 0);
  return Math.max(0, total - Math.max(0, sequence.length - 1) * recipe.crossfade_ms);
}
