// SPDX-License-Identifier: AGPL-3.0-or-later

/** The colors offered for categories and labels. */
export const COLORS = ["#2f6fd6", "#b4549a", "#1d7a3e", "#b07a00", "#c62828", "#0b6e66", "#6b4fbb", "#5f6268"];

/** The first color `used` has not taken yet, or the next one round. */
export function nextColor(used: readonly (string | null | undefined)[]): string {
  const taken = new Set(used.map((color) => color?.toLowerCase()));
  return COLORS.find((color) => !taken.has(color)) ?? COLORS[used.length % COLORS.length] ?? "#2f6fd6";
}
