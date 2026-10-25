// SPDX-License-Identifier: AGPL-3.0-or-later

// Charts of the statistics page: listeners minute by minute (a line, broken
// where minutes are missing) and totals per period (bars). One series each,
// so no legend: the card's title names it. A tooltip follows the pointer;
// the tables next to them hold the same figures.

import { type PointerEvent, type ReactNode, useLayoutEffect, useRef, useState } from "react";
import { number } from "../i18n";

const HEIGHT = 180;
const PAD = { top: 12, right: 12, bottom: 22, left: 36 };

function useWidth() {
  const ref = useRef<HTMLDivElement>(null);
  const [width, setWidth] = useState(600);
  useLayoutEffect(() => {
    const element = ref.current;
    if (!element) {
      return;
    }
    setWidth(element.clientWidth || 600);
    const observer = new ResizeObserver(([entry]) => setWidth(entry?.contentRect.width || 600));
    observer.observe(element);
    return () => observer.disconnect();
  }, []);
  return { ref, width };
}

/** A round top for the y axis, and 3 or 4 steps up to it. */
export function niceScale(max: number): { top: number; ticks: number[] } {
  if (max <= 0) {
    return { top: 1, ticks: [0, 1] };
  }
  const raw = max / 4;
  const magnitude = 10 ** Math.floor(Math.log10(raw));
  const step = [1, 2, 5, 10].map((m) => m * magnitude).find((s) => s >= raw) ?? raw;
  const roundStep = Math.max(step, 1);
  const top = Math.ceil(max / roundStep) * roundStep;
  const ticks = [];
  for (let tick = 0; tick <= top + roundStep / 2; tick += roundStep) {
    ticks.push(tick);
  }
  return { top, ticks };
}

function YAxis({ ticks, y, width }: { ticks: number[]; y: (value: number) => number; width: number }) {
  return (
    <g className="chart-axis">
      {ticks.map((tick) => (
        <g key={tick}>
          <line x1={PAD.left} x2={width - PAD.right} y1={y(tick)} y2={y(tick)} className="chart-grid" />
          <text x={PAD.left - 6} y={y(tick)} dy="0.32em" textAnchor="end">
            {number(tick)}
          </text>
        </g>
      ))}
    </g>
  );
}

function Tooltip({ x, width, children }: { x: number; width: number; children: ReactNode }) {
  // Kept inside the chart: flipped to the left of the pointer near the right edge.
  const right = x > width * 0.65;
  return (
    <div className="chart-tip" style={right ? { right: width - x + 8 } : { left: x + 8 }}>
      {children}
    </div>
  );
}

export interface Point {
  at: number;
  value: number;
}

/** Listeners minute by minute between `from` and `to` (Unix ms). */
export function MinuteChart({
  points,
  from,
  to,
  label,
  tickLabel,
  tooltip,
}: {
  points: Point[];
  from: number;
  to: number;
  label: string;
  tickLabel: (at: number) => string;
  tooltip: (point: Point) => ReactNode;
}) {
  const { ref, width } = useWidth();
  const [hover, setHover] = useState<Point | null>(null);
  const { top, ticks } = niceScale(Math.max(0, ...points.map((p) => p.value)));
  const span = Math.max(to - from, 1);
  const x = (at: number) => PAD.left + ((at - from) / span) * (width - PAD.left - PAD.right);
  const y = (value: number) => PAD.top + (1 - value / top) * (HEIGHT - PAD.top - PAD.bottom);

  // A new segment wherever a minute is missing.
  const segments: Point[][] = [];
  for (const point of points) {
    const segment = segments[segments.length - 1];
    const previous = segment?.[segment.length - 1];
    if (segment && previous && point.at - previous.at <= 60_000) {
      segment.push(point);
    } else {
      segments.push([point]);
    }
  }
  const path = (segment: Point[]) =>
    segment.map((p, i) => `${i === 0 ? "M" : "L"}${x(p.at).toFixed(1)},${y(p.value).toFixed(1)}`).join("");
  const area = (segment: Point[]) => {
    const first = segment[0];
    const last = segment[segment.length - 1];
    if (!first || !last) {
      return "";
    }
    return `${path(segment)}L${x(last.at).toFixed(1)},${y(0)}L${x(first.at).toFixed(1)},${y(0)}Z`;
  };

  // Every few hours from the start (midnight, station time).
  const hours: number[] = [];
  const step = (width < 480 ? 6 : 3) * 3_600_000;
  // Not the end: its label would leave the chart.
  for (let at = from; at < to - step / 2; at += step) {
    hours.push(at);
  }

  const move = (event: PointerEvent<SVGSVGElement>) => {
    const box = event.currentTarget.getBoundingClientRect();
    const at = from + ((event.clientX - box.left - PAD.left) / (width - PAD.left - PAD.right)) * span;
    let nearest: Point | null = null;
    for (const point of points) {
      if (!nearest || Math.abs(point.at - at) < Math.abs(nearest.at - at)) {
        nearest = point;
      }
    }
    setHover(nearest && Math.abs(nearest.at - at) <= 5 * 60_000 ? nearest : null);
  };

  return (
    <div className="chart" ref={ref}>
      <svg
        width={width}
        height={HEIGHT}
        role="img"
        aria-label={label}
        onPointerMove={move}
        onPointerLeave={() => setHover(null)}
      >
        <YAxis ticks={ticks} y={y} width={width} />
        <g className="chart-axis">
          {hours.map((at) => (
            <text key={at} x={x(at)} y={HEIGHT - 6} textAnchor="middle">
              {tickLabel(at)}
            </text>
          ))}
        </g>
        {segments.map((segment) => (
          <g key={segment[0]?.at}>
            <path d={area(segment)} className="chart-area" />
            <path d={path(segment)} className="chart-line" />
            {/* A minute alone between gaps: a dot, a line would not show. */}
            {segment.length === 1 && segment[0] && (
              <circle cx={x(segment[0].at)} cy={y(segment[0].value)} r={2.5} className="chart-dot" />
            )}
          </g>
        ))}
        {hover && (
          <g>
            <line x1={x(hover.at)} x2={x(hover.at)} y1={PAD.top} y2={HEIGHT - PAD.bottom} className="chart-cross" />
            <circle cx={x(hover.at)} cy={y(hover.value)} r={4} className="chart-dot" />
          </g>
        )}
      </svg>
      {hover && (
        <Tooltip x={x(hover.at)} width={width}>
          {tooltip(hover)}
        </Tooltip>
      )}
    </div>
  );
}

export interface Bar {
  key: string;
  /** Under the bar; empty to leave it out where bars are many. */
  label: string;
  value: number;
  tooltip: ReactNode;
}

/** One bar per period. */
export function BarChart({ bars, label }: { bars: Bar[]; label: string }) {
  const { ref, width } = useWidth();
  const [hover, setHover] = useState<number | null>(null);
  const { top, ticks } = niceScale(Math.max(0, ...bars.map((b) => b.value)));
  const inner = width - PAD.left - PAD.right;
  const slot = inner / Math.max(bars.length, 1);
  // A 2px gap between bars, and thin bars when they are few.
  const barWidth = Math.max(1, Math.min(slot - 2, 48));
  const y = (value: number) => PAD.top + (1 - value / top) * (HEIGHT - PAD.top - PAD.bottom);
  const base = y(0);
  const every = Math.max(1, Math.ceil(bars.length / Math.max(1, Math.floor(inner / 56))));
  const hovered = hover === null ? undefined : bars[hover];

  return (
    <div className="chart" ref={ref}>
      <svg width={width} height={HEIGHT} role="img" aria-label={label} onPointerLeave={() => setHover(null)}>
        <YAxis ticks={ticks} y={y} width={width} />
        {bars.map((bar, i) => {
          const center = PAD.left + slot * i + slot / 2;
          const height = Math.max(base - y(bar.value), 0);
          const radius = Math.min(4, barWidth / 2, height);
          const left = center - barWidth / 2;
          const topY = base - height;
          // Rounded at the data end, square on the baseline.
          const d =
            height === 0
              ? ""
              : `M${left},${base}V${topY + radius}Q${left},${topY} ${left + radius},${topY}H${left + barWidth - radius}` +
                `Q${left + barWidth},${topY} ${left + barWidth},${topY + radius}V${base}Z`;
          return (
            <g key={bar.key} onPointerEnter={() => setHover(i)}>
              {/* The whole column answers the pointer, not just the bar. */}
              <rect x={PAD.left + slot * i} y={PAD.top} width={slot} height={base - PAD.top} fill="transparent" />
              <path d={d} className={hover === i ? "chart-bar hover" : "chart-bar"} />
              {bar.label && i % every === 0 && (
                <text x={center} y={HEIGHT - 6} textAnchor="middle" className="chart-axis-text">
                  {bar.label}
                </text>
              )}
            </g>
          );
        })}
      </svg>
      {hovered && hover !== null && (
        <Tooltip x={PAD.left + slot * hover + slot / 2} width={width}>
          {hovered.tooltip}
        </Tooltip>
      )}
    </div>
  );
}
