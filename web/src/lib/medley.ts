// SPDX-License-Identifier: AGPL-3.0-or-later

// Medleys (docs/medleys.md): what the web shows before a render.

import { t } from "../i18n";

/** "Name (copy)", or "Name (copy 2)"… when that name is taken. */
export function copyName(name: string, taken: readonly string[]): string {
  const names = new Set(taken);
  let candidate = t("medleys.copyName", { name });
  for (let n = 2; names.has(candidate); n += 1) {
    candidate = t("medleys.copyNameN", { name, n: String(n) });
  }
  return candidate;
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
