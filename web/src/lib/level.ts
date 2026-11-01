// SPDX-License-Identifier: AGPL-3.0-or-later

// Levels in dBFS, from the waveform: one byte per 50 ms, the highest sample
// peak on a scale from −72 dBFS (0) to full scale (255). See
// docs/analysis.md#job-queue.

import { db } from "./format";

/** The waveform's floor: a byte of 0 is this or below. */
export const FLOOR_DB = -72;

/** A waveform byte in dBFS. */
export function byteDb(byte: number): number {
  return (1 - byte / 255) * FLOOR_DB;
}

/** The level at `ms`, in dBFS; null outside the waveform. */
export function levelAt(peaks: Uint8Array, intervalMs: number, ms: number): number | null {
  const index = Math.floor(ms / Math.max(1, intervalMs));
  const byte = peaks[index];
  return index < 0 || byte === undefined ? null : byteDb(byte);
}

/** `−12.3 dBFS`; `≤ −72.0 dBFS` at the floor; `–` when unknown. */
export function levelText(value: number | null | undefined): string {
  if (value === null || value === undefined) {
    return "–";
  }
  const text = db(value, "dBFS");
  return value <= FLOOR_DB + 0.05 ? `≤ ${text}` : text;
}
