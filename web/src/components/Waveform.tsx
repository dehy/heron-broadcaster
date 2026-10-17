// SPDX-License-Identifier: AGPL-3.0-or-later

// A track's waveform overview with its cue-in, cue-out and fade points.
// Cue points can be dragged; the view can zoom on the intro or the outro.

import { useQuery } from "@tanstack/react-query";
import { type PointerEvent, useEffect, useLayoutEffect, useRef, useState } from "react";
import { ApiError, api, unwrap } from "../api";
import { duration } from "../lib/format";

export interface WaveformData {
  intervalMs: number;
  floorDb: number;
  peaks: Uint8Array;
}

export function decodePeaks(base64: string): Uint8Array {
  const text = atob(base64);
  const peaks = new Uint8Array(text.length);
  for (let i = 0; i < text.length; i += 1) {
    peaks[i] = text.charCodeAt(i);
  }
  return peaks;
}

/** The waveform of a track; `null` until it is analyzed. */
export function useWaveform(trackId: number | undefined) {
  return useQuery({
    queryKey: ["waveform", trackId],
    enabled: trackId !== undefined,
    staleTime: 5 * 60_000,
    queryFn: async (): Promise<WaveformData | null> => {
      try {
        const waveform = await unwrap(
          api.GET("/api/v1/tracks/{id}/waveform", { params: { path: { id: trackId ?? 0 } } }),
        );
        return {
          intervalMs: waveform.interval_ms,
          floorDb: waveform.floor_db,
          peaks: decodePeaks(waveform.peaks),
        };
      } catch (error) {
        if (error instanceof ApiError && error.status === 404) {
          return null;
        }
        throw error;
      }
    },
  });
}

/** Re-renders when the theme changes, so canvases pick up the new colors. */
function useThemeVersion(): number {
  const [version, setVersion] = useState(0);
  useEffect(() => {
    const bump = () => setVersion((v) => v + 1);
    const media = window.matchMedia("(prefers-color-scheme: dark)");
    media.addEventListener("change", bump);
    const observer = new MutationObserver(bump);
    observer.observe(document.documentElement, { attributes: true, attributeFilter: ["data-theme"] });
    return () => {
      media.removeEventListener("change", bump);
      observer.disconnect();
    };
  }, []);
  return version;
}

/** The highest point of `peaks` between two times, 0 to 1. */
function peakBetween(data: WaveformData, fromMs: number, toMs: number): number {
  const first = Math.max(0, Math.floor(fromMs / data.intervalMs));
  const last = Math.min(data.peaks.length - 1, Math.ceil(toMs / data.intervalMs));
  let peak = 0;
  for (let i = first; i <= last; i += 1) {
    peak = Math.max(peak, data.peaks[i] ?? 0);
  }
  return peak / 255;
}

export interface Region {
  startMs: number;
  endMs: number;
  color: string;
  label: string;
}

export interface Marker {
  id: string;
  label: string;
  ms: number;
  color: string;
  /** Flag on the left of the line (an end point). */
  end?: boolean;
  /** Row of the flag, from the top, so close markers stay readable. */
  row?: number;
  /** Called while dragging, then once more with `done`. */
  onDrag?: (ms: number, done: boolean) => void;
}

export function WaveformView({
  data,
  durationMs,
  markers,
  regions = [],
  playheadMs,
  onSeek,
  view,
}: {
  data: WaveformData | null;
  durationMs: number;
  markers: Marker[];
  regions?: Region[];
  playheadMs?: number | null;
  onSeek?: (ms: number) => void;
  /** The part shown, in ms. The whole track by default. */
  view?: [number, number];
}) {
  const box = useRef<HTMLDivElement>(null);
  const canvas = useRef<HTMLCanvasElement>(null);
  const [width, setWidth] = useState(0);
  const theme = useThemeVersion();
  const [from, to] = view ?? [0, durationMs];
  const span = Math.max(1, to - from);
  const cueIn = markers.find((m) => m.id === "cue_in")?.ms ?? 0;
  const cueOut = markers.find((m) => m.id === "cue_out")?.ms ?? durationMs;

  useLayoutEffect(() => {
    const element = box.current;
    if (!element) {
      return;
    }
    const observer = new ResizeObserver(() => setWidth(element.clientWidth));
    observer.observe(element);
    setWidth(element.clientWidth);
    return () => observer.disconnect();
  }, []);

  useEffect(() => {
    const element = canvas.current;
    if (!element || width === 0) {
      return;
    }
    const ratio = window.devicePixelRatio || 1;
    const height = element.clientHeight;
    element.width = Math.round(width * ratio);
    element.height = Math.round(height * ratio);
    const context = element.getContext("2d");
    if (!context) {
      return;
    }
    context.scale(ratio, ratio);
    context.clearRect(0, 0, width, height);
    const style = getComputedStyle(element);
    const color = (name: string) => style.getPropertyValue(name).trim() || "#888";

    for (const region of regions) {
      const x0 = ((region.startMs - from) / span) * width;
      const x1 = ((region.endMs - from) / span) * width;
      context.fillStyle = region.color;
      context.globalAlpha = 0.16;
      context.fillRect(x0, 0, Math.max(1, x1 - x0), height);
    }
    context.globalAlpha = 1;

    const middle = height / 2;
    context.beginPath();
    context.strokeStyle = color("--border");
    context.moveTo(0, middle);
    context.lineTo(width, middle);
    context.stroke();
    if (!data) {
      return;
    }
    const played = color("--wave");
    const skipped = color("--border");
    for (let x = 0; x < width; x += 1) {
      const t0 = from + (x / width) * span;
      const t1 = from + ((x + 1) / width) * span;
      const peak = peakBetween(data, t0, t1);
      const half = Math.max(0.5, peak * (middle - 2));
      context.fillStyle = t1 < cueIn || t0 > cueOut ? skipped : played;
      context.fillRect(x, middle - half, 1, half * 2);
    }
  }, [data, width, from, span, cueIn, cueOut, regions, theme]);

  const msAt = (clientX: number) => {
    const rect = box.current?.getBoundingClientRect();
    if (!rect || rect.width === 0) {
      return 0;
    }
    const ms = from + ((clientX - rect.left) / rect.width) * span;
    return Math.round(Math.min(durationMs, Math.max(0, ms)));
  };
  const left = (ms: number) => `${((ms - from) / span) * 100}%`;
  const visible = (ms: number) => ms >= from && ms <= to;

  return (
    <div className="waveform" ref={box}>
      <canvas
        ref={canvas}
        onClick={(event) => onSeek?.(msAt(event.clientX))}
        style={{ cursor: onSeek ? "pointer" : "default" }}
        aria-hidden="true"
      />
      {!data && (
        <div className="empty" style={{ position: "absolute", inset: 0, padding: "4rem 1rem" }}>
          No waveform yet: it is drawn when the track is analyzed.
        </div>
      )}
      {markers.filter((marker) => visible(marker.ms)).map((marker) => (
        <MarkerLine key={marker.id} marker={marker} left={left(marker.ms)} msAt={msAt} />
      ))}
      {playheadMs !== null && playheadMs !== undefined && visible(playheadMs) && (
        <div className="playhead" style={{ left: left(playheadMs) }} />
      )}
      <div className="axis num">
        <span>{duration(from)}</span>
        <span>{duration(from + span / 2)}</span>
        <span>{duration(to)}</span>
      </div>
    </div>
  );
}

function MarkerLine({
  marker,
  left,
  msAt,
}: {
  marker: Marker;
  left: string;
  msAt: (clientX: number) => number;
}) {
  const dragging = useRef(false);
  const editable = marker.onDrag !== undefined;
  const down = (event: PointerEvent<HTMLButtonElement>) => {
    if (!editable) {
      return;
    }
    dragging.current = true;
    event.currentTarget.setPointerCapture(event.pointerId);
  };
  const move = (event: PointerEvent<HTMLButtonElement>) => {
    if (dragging.current) {
      marker.onDrag?.(msAt(event.clientX), false);
    }
  };
  const up = (event: PointerEvent<HTMLButtonElement>) => {
    if (dragging.current) {
      dragging.current = false;
      marker.onDrag?.(msAt(event.clientX), true);
    }
  };
  const className = [
    "marker",
    marker.end ? "end" : "",
    editable ? "" : "readonly",
  ].join(" ");
  return (
    <div className={className} style={{ left, ["--marker-color" as string]: marker.color }}>
      <button
        type="button"
        className="flag"
        style={{ border: 0, font: "inherit", top: `${(marker.row ?? 0) * 1.4}rem` }}
        title={`${marker.label}: ${duration(marker.ms, true)}${editable ? " (drag, or use the arrow keys)" : ""}`}
        aria-label={`${marker.label} at ${duration(marker.ms, true)}`}
        disabled={!editable}
        onPointerDown={down}
        onPointerMove={move}
        onPointerUp={up}
        onKeyDown={(event) => {
          if (!editable) {
            return;
          }
          const step = event.shiftKey ? 1000 : 100;
          if (event.key === "ArrowLeft" || event.key === "ArrowRight") {
            event.preventDefault();
            const next = marker.ms + (event.key === "ArrowLeft" ? -step : step);
            marker.onDrag?.(Math.max(0, next), true);
          }
        }}
      >
        {marker.label}
      </button>
    </div>
  );
}
