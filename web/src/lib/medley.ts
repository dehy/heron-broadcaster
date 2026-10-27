// SPDX-License-Identifier: AGPL-3.0-or-later

// Medleys (docs/medleys.md): what the web shows before a render.

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
