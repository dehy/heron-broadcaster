// SPDX-License-Identifier: AGPL-3.0-or-later

// Previews of tracks in the browser, from a list: one at a time per page,
// from the track's cue-in to its cue-out. Nothing reaches the station.

import { useCallback, useEffect, useRef, useState } from "react";

/** What a preview needs of a track. */
export interface Previewable {
  id: number;
  /** Where it plays from and to (cue points); start and end of the file when missing. */
  cue_in_ms?: number | null;
  cue_out_ms?: number | null;
}

export interface Preview {
  /** The track playing, or loading. */
  playing: number | null;
  /** Of the track playing, from its cue-in to its cue-out (0 to 1). */
  progress: number;
  /** Tracks the browser could not play (format not supported, file gone). */
  failed: ReadonlySet<number>;
  /** Plays the track, stopping the one playing; stops it when it is the one playing. */
  toggle: (track: Previewable) => void;
  stop: () => void;
}

/**
 * One preview at a time for the component using it, stopped when it
 * unmounts (the page changes).
 */
export function usePreview(): Preview {
  const audio = useRef<HTMLAudioElement | null>(null);
  const range = useRef<{ from: number; to: number | null }>({ from: 0, to: null });
  const [playing, setPlaying] = useState<number | null>(null);
  const [progress, setProgress] = useState(0);
  const [failed, setFailed] = useState<ReadonlySet<number>>(new Set());

  const stop = useCallback(() => {
    const element = audio.current;
    if (element) {
      element.pause();
      element.removeAttribute("src");
      element.load();
    }
    setPlaying(null);
    setProgress(0);
  }, []);

  // Follows the audio while it plays, and stops at the cue-out.
  useEffect(() => {
    if (playing === null) {
      return;
    }
    let frame = 0;
    const step = () => {
      const element = audio.current;
      if (element) {
        const position = element.currentTime * 1000;
        const { from, to } = range.current;
        const end = to ?? (Number.isFinite(element.duration) ? element.duration * 1000 : null);
        if (to !== null && position >= to) {
          stop();
          return;
        }
        setProgress(end !== null && end > from ? Math.min(1, Math.max(0, (position - from) / (end - from))) : 0);
      }
      frame = requestAnimationFrame(step);
    };
    frame = requestAnimationFrame(step);
    return () => cancelAnimationFrame(frame);
  }, [playing, stop]);

  useEffect(
    () => () => {
      audio.current?.pause();
      audio.current = null;
    },
    [],
  );

  const toggle = useCallback(
    (track: Previewable) => {
      if (playing === track.id) {
        stop();
        return;
      }
      const element = audio.current ?? new Audio();
      audio.current = element;
      const fail = () => {
        setFailed((previous) => new Set(previous).add(track.id));
        setPlaying((current) => (current === track.id ? null : current));
      };
      element.onended = () => stop();
      element.onerror = fail;
      element.pause();
      const from = track.cue_in_ms ?? 0;
      range.current = { from, to: track.cue_out_ms ?? null };
      // From the cue-in: a media fragment, so the browser asks for that part first.
      element.src = `/api/v1/tracks/${track.id}/audio#t=${from / 1000}`;
      element.currentTime = from / 1000;
      setPlaying(track.id);
      setProgress(0);
      setFailed((previous) => {
        if (!previous.has(track.id)) {
          return previous;
        }
        const next = new Set(previous);
        next.delete(track.id);
        return next;
      });
      element.play().catch((error: unknown) => {
        // A newer preview took over (AbortError): nothing went wrong.
        if (error instanceof DOMException && error.name === "AbortError") {
          return;
        }
        fail();
      });
    },
    [playing, stop],
  );

  return { playing, progress, failed, toggle, stop };
}
