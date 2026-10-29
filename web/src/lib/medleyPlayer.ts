// SPDX-License-Identifier: AGPL-3.0-or-later

// The medley page's one player (docs/medleys.md): the rendered medley, the
// preview of the version being edited and each part play through the same
// audio element, so starting one stops the others.

import { useCallback, useEffect, useRef, useState } from "react";

export type PlayerSource =
  /** A whole medley file: its time is the medley's timeline. */
  | { kind: "medley"; url: string }
  /** One part: a span of its track's file. */
  | { kind: "part"; index: number; track_id: number; from_ms: number; to_ms: number };

export interface MedleyPlayer {
  source: PlayerSource | null;
  playing: boolean;
  /** Where it is: on the medley's timeline, or in the part's file. */
  position_ms: number;
  /** The file's length, once known (a whole medley). */
  length_ms: number | null;
  failed: boolean;
  /** Plays `source` from `at_ms` (its start when missing), stopping what plays. */
  play: (source: PlayerSource, at_ms?: number) => void;
  pause: () => void;
  resume: () => void;
  stop: () => void;
}

const same = (a: PlayerSource | null, b: PlayerSource) =>
  a !== null &&
  (a.kind === "medley" && b.kind === "medley"
    ? a.url === b.url
    : a.kind === "part" && b.kind === "part" && a.index === b.index && a.track_id === b.track_id);

/** One player for the component using it, stopped when it unmounts. */
export function useMedleyPlayer(): MedleyPlayer {
  const audio = useRef<HTMLAudioElement | null>(null);
  const [source, setSource] = useState<PlayerSource | null>(null);
  const [playing, setPlaying] = useState(false);
  const [position, setPosition] = useState(0);
  const [length, setLength] = useState<number | null>(null);
  const [failed, setFailed] = useState(false);
  const current = useRef<PlayerSource | null>(null);

  const stop = useCallback(() => {
    const element = audio.current;
    if (element) {
      element.pause();
      element.removeAttribute("src");
      element.load();
    }
    current.current = null;
    setSource(null);
    setPlaying(false);
    setPosition(0);
    setLength(null);
  }, []);

  // Follows the audio while it plays; a part stops at its end.
  useEffect(() => {
    if (!playing) {
      return;
    }
    let frame = 0;
    const step = () => {
      const element = audio.current;
      const playingNow = current.current;
      if (element && playingNow) {
        const ms = element.currentTime * 1000;
        if (playingNow.kind === "part" && ms >= playingNow.to_ms) {
          stop();
          return;
        }
        setPosition(ms);
        if (Number.isFinite(element.duration)) {
          setLength(element.duration * 1000);
        }
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

  const play = useCallback(
    (next: PlayerSource, at_ms?: number) => {
      const element = audio.current ?? new Audio();
      audio.current = element;
      const from = at_ms ?? (next.kind === "part" ? next.from_ms : 0);
      element.onended = () => stop();
      element.onerror = () => {
        setFailed(true);
        setPlaying(false);
      };
      if (!same(current.current, next)) {
        element.pause();
        const url = next.kind === "medley" ? next.url : `/api/v1/tracks/${next.track_id}/audio`;
        element.src = `${url}#t=${from / 1000}`;
        setLength(null);
      }
      element.currentTime = from / 1000;
      current.current = next;
      setSource(next);
      setPosition(from);
      setFailed(false);
      setPlaying(true);
      element.play().catch((error: unknown) => {
        // A newer source took over (AbortError): nothing went wrong.
        if (error instanceof DOMException && error.name === "AbortError") {
          return;
        }
        setFailed(true);
        setPlaying(false);
      });
    },
    [stop],
  );

  const pause = useCallback(() => {
    audio.current?.pause();
    setPlaying(false);
  }, []);

  const resume = useCallback(() => {
    const element = audio.current;
    if (element && current.current) {
      setPlaying(true);
      element.play().catch(() => setPlaying(false));
    }
  }, []);

  return { source, playing, position_ms: position, length_ms: length, failed, play, pause, resume, stop };
}
