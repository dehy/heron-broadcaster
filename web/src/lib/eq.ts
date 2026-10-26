// SPDX-License-Identifier: AGPL-3.0-or-later

// The deck EQ: its bands, the response curve the engine's filters have
// (the RBJ cookbook's peaking EQ, as in heron_dsp::eq), and how slider
// moves are sent: the latest value, one request at a time, at most every
// 100 ms. See docs/audio-engine.md#deck-eq.

import { useQuery } from "@tanstack/react-query";
import { type DeckEq, type EqSettings, api, unwrap } from "../api";
import { useLive } from "./live";

/** Centre frequency of each band, in Hz. */
export const FREQUENCIES = [60, 170, 310, 600, 1_000, 3_000, 6_000, 12_000, 14_000, 16_000];
export const MAX_GAIN_DB = 12;
const Q = Math.SQRT2;
const SAMPLE_RATE = 48_000;

export const EQ_KEY = ["settings", "eq"] as const;

/**
 * The deck EQ: as read, then as the live WebSocket reports it changing.
 * Read again on reconnection.
 */
export function useEqSettings(): { data: EqSettings | undefined; error: unknown } {
  const connected = useLive((live) => live.connected);
  const live = useLive((live) => live.eq);
  const query = useQuery({
    queryKey: [...EQ_KEY, connected],
    queryFn: () => unwrap(api.GET("/api/v1/settings/eq")),
  });
  return { data: live ?? query.data, error: query.error };
}

/** "60", "1k", "12k". */
export function bandLabel(frequency: number): string {
  return frequency >= 1_000 ? `${frequency / 1_000}k` : String(frequency);
}

/** Gain of one peaking band at `frequency`, in dB. */
function bandDb(centre: number, gainDb: number, frequency: number): number {
  const a = 10 ** (gainDb / 40);
  const w0 = (2 * Math.PI * centre) / SAMPLE_RATE;
  const alpha = Math.sin(w0) / (2 * Q);
  const cos = Math.cos(w0);
  const [b0, b1, b2] = [1 + alpha * a, -2 * cos, 1 - alpha * a];
  const [a0, a1, a2] = [1 + alpha / a, -2 * cos, 1 - alpha / a];
  const w = (2 * Math.PI * frequency) / SAMPLE_RATE;
  const magnitude = (c0: number, c1: number, c2: number) => {
    const re = c0 + c1 * Math.cos(w) + c2 * Math.cos(2 * w);
    const im = -(c1 * Math.sin(w) + c2 * Math.sin(2 * w));
    return re * re + im * im;
  };
  return 10 * Math.log10(magnitude(b0, b1, b2) / magnitude(a0, a1, a2));
}

/** Gain of the whole EQ at `frequency`, in dB. */
export function responseDb(gains: number[], frequency: number): number {
  return FREQUENCIES.reduce((sum, centre, band) => sum + bandDb(centre, gains[band] ?? 0, frequency), 0);
}

/** The EQ changes the sound: on, with a band away from 0 dB. */
export function isShaping(eq: DeckEq | undefined | null): boolean {
  return !!eq?.enabled && eq.gains_db.some((gain) => gain !== 0);
}

/** A gain within ±12 dB, to the nearest 0.5 dB. */
export function roundGain(gain: number): number {
  return Math.max(-MAX_GAIN_DB, Math.min(MAX_GAIN_DB, Math.round(gain * 2) / 2));
}

/**
 * Sends values as they change: one request at a time, at most every
 * `intervalMs`, and only the latest value: those in between are skipped.
 */
export function latestSender<T>(send: (value: T) => Promise<unknown>, intervalMs = 100) {
  let pending: { value: T } | null = null;
  let busy = false;
  let lastSent = Number.NEGATIVE_INFINITY;
  let onError: (error: unknown) => void = () => {};

  const pump = async () => {
    if (busy || !pending) {
      return;
    }
    busy = true;
    const wait = lastSent + intervalMs - Date.now();
    if (wait > 0) {
      await new Promise((resolve) => setTimeout(resolve, wait));
    }
    const next = pending;
    pending = null;
    lastSent = Date.now();
    try {
      if (next) {
        await send(next.value);
      }
    } catch (error) {
      onError(error);
    } finally {
      busy = false;
    }
    void pump();
  };

  return {
    push(value: T) {
      pending = { value };
      void pump();
    },
    /** Waiting to send, or sending. */
    get active() {
      return busy || pending !== null;
    },
    set onError(handler: (error: unknown) => void) {
      onError = handler;
    },
  };
}
