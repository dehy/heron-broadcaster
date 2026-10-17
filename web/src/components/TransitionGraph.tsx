// SPDX-License-Identifier: AGPL-3.0-or-later

// The gains of two tracks around their transition, as the engine plans it.

import type { TransitionPreview } from "../api";
import { incomingGain, outgoingGain } from "../lib/fade";
import { duration } from "../lib/format";
import type { WaveformData } from "./Waveform";

const W = 720;
const H = 200;
const PAD = { top: 12, right: 12, bottom: 26, left: 40 };
const STEPS = 240;

function peakAt(data: WaveformData | null | undefined, ms: number): number {
  if (!data || ms < 0) {
    return 0;
  }
  return (data.peaks[Math.floor(ms / data.intervalMs)] ?? 0) / 255;
}

export function TransitionGraph({
  preview,
  outgoingWave,
  incomingWave,
}: {
  preview: TransitionPreview;
  outgoingWave?: WaveformData | null;
  incomingWave?: WaveformData | null;
}) {
  const out = preview.outgoing;
  const into = preview.incoming;
  // A window around the transition, on the outgoing track's timeline.
  const fadeStart = preview.fade_out?.start_ms ?? preview.next_start_ms;
  const from = Math.max(out.cue_in_ms, Math.min(fadeStart, preview.next_start_ms) - 4_000);
  const to = Math.max(out.cue_out_ms, preview.next_start_ms + (preview.fade_in?.length_ms ?? 0)) + 4_000;
  const span = Math.max(1, to - from);
  const plotW = W - PAD.left - PAD.right;
  const plotH = H - PAD.top - PAD.bottom;
  const x = (ms: number) => PAD.left + ((ms - from) / span) * plotW;
  const y = (gain: number) => PAD.top + (1 - gain) * plotH;
  // The incoming track's own time at `ms` on the outgoing timeline.
  const incomingTime = (ms: number) => ms - preview.next_start_ms + into.cue_in_ms;

  const points = (gain: (ms: number) => number) =>
    Array.from({ length: STEPS + 1 }, (_, i) => {
      const ms = from + (i / STEPS) * span;
      return `${x(ms).toFixed(1)},${y(gain(ms)).toFixed(1)}`;
    }).join(" ");
  const outGain = (ms: number) => (ms < out.cue_in_ms ? 0 : outgoingGain(preview.fade_out, out.cue_out_ms, ms));
  const inGain = (ms: number) =>
    ms < preview.next_start_ms ? 0 : incomingGain(preview.fade_in, into.cue_in_ms, incomingTime(ms));

  // Waveforms scaled by their gain, as heard.
  const wave = (data: WaveformData | null | undefined, time: (ms: number) => number, gain: (ms: number) => number) => {
    if (!data) {
      return "";
    }
    const top: string[] = [];
    const bottom: string[] = [];
    for (let i = 0; i <= STEPS; i += 1) {
      const ms = from + (i / STEPS) * span;
      const level = peakAt(data, time(ms)) * gain(ms) * 0.5;
      top.push(`${x(ms).toFixed(1)},${(PAD.top + plotH / 2 - level * plotH).toFixed(1)}`);
      bottom.unshift(`${x(ms).toFixed(1)},${(PAD.top + plotH / 2 + level * plotH).toFixed(1)}`);
    }
    return [...top, ...bottom].join(" ");
  };

  const ticks = [0, 0.5, 1];
  const timeTicks = [from, preview.next_start_ms, out.cue_out_ms, to].filter(
    (ms, i, all) => all.findIndex((other) => Math.abs(other - ms) < span / 12) === i,
  );

  return (
    <figure style={{ margin: 0 }}>
      <svg
        className="graph"
        viewBox={`0 0 ${W} ${H}`}
        role="img"
        aria-label={`The next track starts ${duration(out.cue_out_ms - preview.next_start_ms, true)} before the outgoing one ends`}
      >
        {ticks.map((gain) => (
          <g key={gain}>
            <line x1={PAD.left} x2={W - PAD.right} y1={y(gain)} y2={y(gain)} stroke="var(--border)" />
            <text x={PAD.left - 6} y={y(gain) + 4} textAnchor="end" fontSize="11" fill="var(--muted)">
              {Math.round(gain * 100)}%
            </text>
          </g>
        ))}
        <polygon points={wave(outgoingWave, (ms) => ms, outGain)} fill="var(--deck-a)" opacity="0.18" />
        <polygon points={wave(incomingWave, incomingTime, inGain)} fill="var(--deck-b)" opacity="0.18" />
        {preview.overlap_ms > 0 && (
          <rect
            x={x(preview.next_start_ms)}
            y={PAD.top}
            width={Math.max(1, x(out.cue_out_ms) - x(preview.next_start_ms))}
            height={plotH}
            fill="var(--accent)"
            opacity="0.08"
          />
        )}
        <polyline points={points(outGain)} fill="none" stroke="var(--deck-a)" strokeWidth="2.5" />
        <polyline points={points(inGain)} fill="none" stroke="var(--deck-b)" strokeWidth="2.5" />
        <line
          x1={x(preview.next_start_ms)}
          x2={x(preview.next_start_ms)}
          y1={PAD.top}
          y2={PAD.top + plotH}
          stroke="var(--accent)"
          strokeDasharray="4 3"
        />
        <line
          x1={x(out.cue_out_ms)}
          x2={x(out.cue_out_ms)}
          y1={PAD.top}
          y2={PAD.top + plotH}
          stroke="var(--bad)"
          strokeDasharray="4 3"
        />
        {timeTicks.map((ms, index) => (
          <text
            key={ms}
            x={x(ms)}
            y={H - 8}
            textAnchor={index === 0 ? "start" : index === timeTicks.length - 1 ? "end" : "middle"}
            fontSize="11"
            fill="var(--muted)"
          >
            {duration(ms, true)}
          </text>
        ))}
      </svg>
      <figcaption className="legend muted">
        <span>
          <span className="swatch" style={{ background: "var(--deck-a)" }} /> Outgoing
        </span>
        <span>
          <span className="swatch" style={{ background: "var(--deck-b)" }} /> Incoming
        </span>
        <span>
          <span className="swatch" style={{ background: "var(--accent)" }} /> Next starts
        </span>
        <span>
          <span className="swatch" style={{ background: "var(--bad)" }} /> Cue out
        </span>
        <span>
          {preview.overlap_ms > 0
            ? `Both play for ${duration(preview.overlap_ms, true)}`
            : "Back to back, no overlap"}
        </span>
      </figcaption>
    </figure>
  );
}
