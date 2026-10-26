// SPDX-License-Identifier: AGPL-3.0-or-later

// Plays the transition between two tracks in the browser, the way the
// engine would mix it: each file streamed from the library through its own
// gain (loudness normalization and its fade), the incoming track started at
// its planned time. A preview only: the station hears nothing.

import { useCallback, useEffect, useRef, useState } from "react";
import type { TransitionPreview } from "../api";
import { transitionWindow } from "../components/TransitionGraph";
import { incomingGain, outgoingGain } from "./fade";

/** Gain points per second of the automation curves. */
const CURVE_RATE = 100;
/** Time given to the tracks to start, before the mix begins. */
const LEAD_S = 0.15;

export type TransitionPlayer = {
  /** Where the mix is, on the outgoing track's timeline; null when stopped. */
  position: number | null;
  /** Loading the files, before the mix starts. */
  loading: boolean;
  play: (outgoingId: number, incomingId: number, preview: TransitionPreview) => void;
  stop: () => void;
};

/** Linear gain of `db` decibels. */
function linear(db: number): number {
  return 10 ** (db / 20);
}

/** The element's file, ready to play from `seconds`. */
function seek(element: HTMLAudioElement, seconds: number): Promise<void> {
  return new Promise((resolve, reject) => {
    const failed = () => reject(new Error(element.error?.message || "the file cannot be played"));
    element.addEventListener("error", failed, { once: true });
    const ready = () => {
      element.removeEventListener("error", failed);
      resolve();
    };
    const go = () => {
      element.addEventListener("seeked", ready, { once: true });
      element.currentTime = seconds;
    };
    if (element.readyState >= HTMLMediaElement.HAVE_METADATA) {
      go();
    } else {
      element.addEventListener("loadedmetadata", go, { once: true });
    }
  });
}

export function useTransitionPlayer(onError: (error: unknown) => void): TransitionPlayer {
  const [position, setPosition] = useState<number | null>(null);
  const [loading, setLoading] = useState(false);
  const session = useRef<(() => void) | null>(null);

  const stop = useCallback(() => {
    session.current?.();
    session.current = null;
    setPosition(null);
    setLoading(false);
  }, []);
  useEffect(() => stop, [stop]);

  const play = useCallback(
    (outgoingId: number, incomingId: number, preview: TransitionPreview) => {
      stop();
      const { from, to } = transitionWindow(preview);
      const out = preview.outgoing;
      const into = preview.incoming;
      const total = (to - from) / 1000;

      // Created in the click, so the browser lets it play.
      const context = new AudioContext();
      // The tracks' gains can add up while both play: a limiter keeps the
      // preview from clipping.
      const limiter = context.createDynamicsCompressor();
      limiter.threshold.value = -1;
      limiter.knee.value = 0;
      limiter.ratio.value = 20;
      limiter.attack.value = 0.003;
      limiter.release.value = 0.1;
      limiter.connect(context.destination);
      const track = (id: number) => {
        const element = new Audio();
        element.preload = "auto";
        element.src = `/api/v1/tracks/${id}/audio`;
        const gain = context.createGain();
        gain.gain.value = 0;
        context.createMediaElementSource(element).connect(gain).connect(limiter);
        return { element, gain };
      };
      const a = track(outgoingId);
      const b = track(incomingId);

      let stopped = false;
      const timers: number[] = [];
      let frame = 0;
      session.current = () => {
        stopped = true;
        timers.forEach((timer) => clearTimeout(timer));
        cancelAnimationFrame(frame);
        for (const { element } of [a, b]) {
          element.pause();
          element.removeAttribute("src");
          element.load();
        }
        void context.close();
      };
      setLoading(true);

      void (async () => {
        try {
          await context.resume();
          await Promise.all([seek(a.element, from / 1000), seek(b.element, into.cue_in_ms / 1000)]);
          if (stopped) {
            return;
          }
          // Gains over the window, sampled on the outgoing track's timeline.
          const points = Math.max(2, Math.ceil(total * CURVE_RATE));
          const curve = (gain: (ms: number) => number) =>
            Float32Array.from({ length: points }, (_, i) => gain(from + (i / (points - 1)) * (to - from)));
          const outLevel = linear(out.gain_db);
          const inLevel = linear(into.gain_db);
          const start = context.currentTime + LEAD_S;
          a.gain.gain.setValueCurveAtTime(
            curve((ms) => (ms < out.cue_in_ms ? 0 : outgoingGain(preview.fade_out, out.cue_out_ms, ms)) * outLevel),
            start,
            total,
          );
          b.gain.gain.setValueCurveAtTime(
            curve((ms) =>
              ms < preview.next_start_ms
                ? 0
                : incomingGain(preview.fade_in, into.cue_in_ms, ms - preview.next_start_ms + into.cue_in_ms) *
                  inLevel,
            ),
            start,
            total,
          );
          const at = (ms: number) => Math.max(0, (start - context.currentTime) * 1000 + ms);
          timers.push(window.setTimeout(() => void a.element.play().catch(onError), at(0)));
          timers.push(window.setTimeout(() => void b.element.play().catch(onError), at(preview.next_start_ms - from)));
          timers.push(window.setTimeout(() => a.element.pause(), at(out.cue_out_ms - from)));
          timers.push(window.setTimeout(stop, at(to - from)));
          setLoading(false);
          const step = () => {
            const elapsed = context.currentTime - start;
            setPosition(elapsed < 0 ? from : from + elapsed * 1000);
            frame = requestAnimationFrame(step);
          };
          frame = requestAnimationFrame(step);
        } catch (error) {
          if (!stopped) {
            stop();
            onError(error);
          }
        }
      })();
    },
    [stop, onError],
  );

  return { position, loading, play, stop };
}
