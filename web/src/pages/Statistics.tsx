// SPDX-License-Identifier: AGPL-3.0-or-later

// Listener statistics, from the stream servers: today live, then by day,
// week or month, the hours of the day, the tracks listeners leave and the
// players (see docs/statistics.md).

import { useQuery } from "@tanstack/react-query";
import { type ReactNode, useState } from "react";
import { type Schemas, api, query, unwrap } from "../api";
import { type Bar, BarChart, MinuteChart } from "../components/ListenerChart";
import { Card, Empty, ErrorView, Loading, PageHead, Segmented } from "../components/ui";
import { type PlainKey, locale, number, t } from "../i18n";
import { clockTime, duration } from "../lib/format";
import { addDays, dayOf, dayStart, percent, useListenersNow } from "../lib/listeners";
import { Link } from "../lib/router";

type Grouping = "day" | "week" | "month";
type Measure = "listening_hours" | "peak" | "average" | "distinct_listeners";
type PeriodTotals = Schemas["PeriodTotals"];

const RANGES = [7, 30, 90, 365] as const;

const MEASURES: { value: Measure; label: PlainKey }[] = [
  { value: "listening_hours", label: "statistics.listeningHours" },
  { value: "peak", label: "statistics.peak" },
  { value: "average", label: "statistics.average" },
  { value: "distinct_listeners", label: "statistics.listeners" },
];

function decimal(value: number | null | undefined, digits = 1): string {
  return value === null || value === undefined ? "–" : number(value, { maximumFractionDigits: digits });
}

/** A day (`YYYY-MM-DD`) written short, `Oct 19`. */
function shortDay(day: string, options: Intl.DateTimeFormatOptions = { month: "short", day: "numeric" }): string {
  const [y, m, d] = day.split("-").map(Number);
  return new Intl.DateTimeFormat(locale(), { ...options, timeZone: "UTC" }).format(
    new Date(Date.UTC(y ?? 1970, (m ?? 1) - 1, d ?? 1)),
  );
}

function periodLabel(day: string, grouping: Grouping): string {
  switch (grouping) {
    case "week":
      return t("statistics.weekOf", { day: shortDay(day) });
    case "month":
      return shortDay(day, { month: "long", year: "numeric" });
    default:
      return shortDay(day, { weekday: "short", month: "short", day: "numeric" });
  }
}

function csvUrl(path: string, params: Record<string, string | number | undefined>): string {
  const search = new URLSearchParams({ format: "csv" });
  for (const [key, value] of Object.entries(params)) {
    if (value !== undefined) {
      search.set(key, String(value));
    }
  }
  return `/api/v1/listeners/${path}?${search.toString()}`;
}

function Csv({ href }: { href: string }) {
  return (
    <a className="btn small" href={href} download>
      {t("statistics.csv")}
    </a>
  );
}

export function Statistics() {
  const station = useQuery({
    queryKey: ["settings", "station"],
    queryFn: () => unwrap(api.GET("/api/v1/settings/station")),
  });
  const outputs = useQuery({ queryKey: ["outputs"], queryFn: () => unwrap(api.GET("/api/v1/outputs")) });
  const [output, setOutput] = useState<number | undefined>(undefined);
  const zone = station.data?.timezone;
  return (
    <>
      <PageHead
        title={t("nav.statistics")}
        description={t("statistics.description", { zone: zone ?? "…" })}
        actions={
          <select
            className="input"
            aria-label={t("statistics.output")}
            value={output ?? ""}
            onChange={(e) => setOutput(e.target.value === "" ? undefined : Number(e.target.value))}
          >
            <option value="">{t("statistics.wholeStation")}</option>
            {outputs.data?.map((o) => (
              <option key={o.id} value={o.id}>
                {o.name}
              </option>
            ))}
          </select>
        }
      />
      {station.isPending ? (
        <Loading />
      ) : (
        <div className="stack">
          <Today zone={zone} output={output} />
          <OverTime zone={zone} output={output} />
          <HoursOfDay zone={zone} output={output} />
          <Departures zone={zone} />
          <Players zone={zone} output={output} />
        </div>
      )}
    </>
  );
}

function Tile({ label, value, hint }: { label: string; value: ReactNode; hint?: string | undefined }) {
  return (
    <div className="stat" title={hint}>
      <span className="value">{value}</span>
      <span className="label">{label}</span>
    </div>
  );
}

function Today({ zone, output }: { zone: string | undefined; output: number | undefined }) {
  const now = useListenersNow();
  const today = dayOf(Date.now(), zone);
  const from = dayStart(today, zone);
  const to = dayStart(addDays(today, 1), zone);
  const samples = useQuery({
    queryKey: ["listeners", "samples", from, output],
    queryFn: () =>
      unwrap(
        api.GET("/api/v1/listeners/samples", {
          params: { query: { from_unix_ms: from, to_unix_ms: to, ...query({ output_id: output }) } },
        }),
      ),
    refetchInterval: 60_000,
  });
  const totals = useQuery({
    queryKey: ["listeners", "days", today, today, output, "day"],
    queryFn: () =>
      unwrap(
        api.GET("/api/v1/listeners/days", { params: { query: { from: today, to: today, ...query({ output_id: output }) } } }),
      ),
    refetchInterval: 60_000,
  });
  const current =
    output === undefined ? now.data?.total : now.data?.outputs.find((o) => o.output_id === output)?.listeners;
  const day = totals.data?.[0];
  const unread = now.data?.outputs.filter((o) => o.error && (output === undefined || o.output_id === output)) ?? [];

  return (
    <Card title={t("statistics.today")}>
      <div className="stack">
        {unread.map((o) => (
          <p key={o.output_id} className="warn small" style={{ margin: 0 }}>
            {t("statistics.unread", { name: o.name, error: o.error ?? "" })}
          </p>
        ))}
        <div className="stats-tiles">
          <Tile label={t("statistics.now")} value={current === undefined || current === null ? "–" : number(current)} />
          <Tile label={t("statistics.peak")} value={day ? number(day.peak) : "–"} />
          <Tile label={t("statistics.listeningHours")} value={decimal(day?.listening_hours)} />
          <Tile
            label={t("statistics.listeners")}
            value={decimal(day?.distinct_listeners, 0)}
            hint={day && day.distinct_listeners == null ? t("statistics.noSessions") : undefined}
          />
          <Tile
            label={t("statistics.averageSession")}
            value={day?.average_session_s == null ? "–" : duration(day.average_session_s * 1000)}
          />
        </div>
        {samples.isError && <ErrorView error={samples.error} />}
        {samples.data &&
          (samples.data.length === 0 ? (
            <Empty>{t("statistics.noSamples")}</Empty>
          ) : (
            <MinuteChart
              label={t("statistics.todayChart")}
              points={samples.data.map((s) => ({ at: s.at_unix_ms, value: s.listeners }))}
              from={from}
              to={to}
              tickLabel={(at) => clockTime(at).replace(/:\d\d$/, "")}
              tooltip={(point) => (
                <>
                  <strong>{number(point.value)}</strong> · {clockTime(point.at).replace(/:\d\d$/, "")}
                </>
              )}
            />
          ))}
        {day && day.distinct_listeners == null && <p className="muted small">{t("statistics.noSessionsHelp")}</p>}
      </div>
    </Card>
  );
}

function RangeSelect({ value, onChange }: { value: number; onChange: (days: number) => void }) {
  return (
    <select className="input" aria-label={t("statistics.range")} value={value} onChange={(e) => onChange(Number(e.target.value))}>
      {RANGES.map((days) => (
        <option key={days} value={days}>
          {t("statistics.lastDays", { count: days })}
        </option>
      ))}
    </select>
  );
}

function measureText(row: PeriodTotals, measure: Measure): string {
  switch (measure) {
    case "peak":
      return number(row.peak);
    case "average":
      return decimal(row.average);
    case "distinct_listeners":
      return decimal(row.distinct_listeners);
    default:
      return decimal(row.listening_hours);
  }
}

function OverTime({ zone, output }: { zone: string | undefined; output: number | undefined }) {
  const [grouping, setGrouping] = useState<Grouping>("day");
  const [range, setRange] = useState(30);
  const [measure, setMeasure] = useState<Measure>("listening_hours");
  const to = dayOf(Date.now(), zone);
  const from = addDays(to, 1 - range);
  const totals = useQuery({
    queryKey: ["listeners", "days", from, to, output, grouping],
    queryFn: () =>
      unwrap(
        api.GET("/api/v1/listeners/days", {
          params: { query: { from, to, ...query({ output_id: output, group: grouping }) } },
        }),
      ),
  });
  const rows = totals.data ?? [];
  const bars: Bar[] = rows.map((row) => ({
    key: row.day,
    label: grouping === "month" ? shortDay(row.day, { month: "short" }) : shortDay(row.day),
    value: row[measure] ?? 0,
    tooltip: (
      <>
        <strong>{measureText(row, measure)}</strong> · {periodLabel(row.day, grouping)}
      </>
    ),
  }));
  const measureLabel = t(MEASURES.find((m) => m.value === measure)?.label ?? "statistics.listeningHours");

  return (
    <Card
      title={t("statistics.overTime")}
      actions={<Csv href={csvUrl("days", { from, to, output_id: output, group: grouping })} />}
    >
      <div className="stack">
        <div className="row">
          <RangeSelect value={range} onChange={setRange} />
          <Segmented
            label={t("statistics.grouping")}
            value={grouping}
            onChange={setGrouping}
            options={[
              { value: "day", label: t("statistics.byDay") },
              { value: "week", label: t("statistics.byWeek") },
              { value: "month", label: t("statistics.byMonth") },
            ]}
          />
          <Segmented
            label={t("statistics.measure")}
            value={measure}
            onChange={setMeasure}
            options={MEASURES.map((m) => ({ value: m.value, label: t(m.label) }))}
          />
        </div>
        {totals.isPending && <Loading />}
        {totals.isError && <ErrorView error={totals.error} />}
        {totals.data && rows.length === 0 && <Empty>{t("statistics.noData")}</Empty>}
        {rows.length > 0 && (
          <>
            <BarChart bars={bars} label={`${t("statistics.overTime")}: ${measureLabel}`} />
            {grouping !== "day" && measure === "distinct_listeners" && (
              <p className="muted small" style={{ margin: 0 }}>
                {t("statistics.perDayHelp")}
              </p>
            )}
            <div className="table-wrap">
              <table className="table">
                <thead>
                  <tr>
                    <th>{t("statistics.period")}</th>
                    <th className="num">{t("statistics.listeners")}</th>
                    <th className="num">{t("statistics.peak")}</th>
                    <th className="num hide-narrow">{t("statistics.average")}</th>
                    <th className="num">{t("statistics.listeningHours")}</th>
                    <th className="num hide-narrow">{t("statistics.sessions")}</th>
                    <th className="num hide-narrow">{t("statistics.averageSession")}</th>
                  </tr>
                </thead>
                <tbody>
                  {[...rows].reverse().map((row) => (
                    <tr key={row.day}>
                      <td>{periodLabel(row.day, grouping)}</td>
                      <td className="num">{decimal(row.distinct_listeners, grouping === "day" ? 0 : 1)}</td>
                      <td className="num">{number(row.peak)}</td>
                      <td className="num hide-narrow">{decimal(row.average)}</td>
                      <td className="num">{decimal(row.listening_hours)}</td>
                      <td className="num hide-narrow">{row.sessions == null ? "–" : number(row.sessions)}</td>
                      <td className="num hide-narrow">
                        {row.average_session_s == null ? "–" : duration(row.average_session_s * 1000)}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </>
        )}
      </div>
    </Card>
  );
}

function HoursOfDay({ zone, output }: { zone: string | undefined; output: number | undefined }) {
  const [range, setRange] = useState(30);
  const to = dayOf(Date.now(), zone);
  const from = addDays(to, 1 - range);
  const hours = useQuery({
    queryKey: ["listeners", "hours", from, to, output],
    queryFn: () =>
      unwrap(api.GET("/api/v1/listeners/hours-of-day", { params: { query: { from, to, ...query({ output_id: output }) } } })),
  });
  const label = (hour: number) => `${String(hour).padStart(2, "0")}h`;
  const bars: Bar[] = Array.from({ length: 24 }, (_, hour) => {
    const row = hours.data?.find((h) => h.hour === hour);
    return {
      key: String(hour),
      label: hour % 3 === 0 ? label(hour) : "",
      value: row?.average ?? 0,
      tooltip: row ? (
        <>
          <strong>{decimal(row.average)}</strong> {t("statistics.onAverage")} · {label(hour)}
          <br />
          {t("statistics.peak")} {number(row.peak)} · {t("statistics.listeningHours")} {decimal(row.listening_hours)}
        </>
      ) : (
        <>
          {label(hour)} · {t("statistics.notMeasured")}
        </>
      ),
    };
  });
  return (
    <Card
      title={t("statistics.hoursOfDay")}
      actions={
        <>
          <RangeSelect value={range} onChange={setRange} />
          <Csv href={csvUrl("hours-of-day", { from, to, output_id: output })} />
        </>
      }
    >
      {hours.isPending && <Loading />}
      {hours.isError && <ErrorView error={hours.error} />}
      {hours.data &&
        (hours.data.length === 0 ? (
          <Empty>{t("statistics.noData")}</Empty>
        ) : (
          <>
            <BarChart bars={bars} label={t("statistics.hoursOfDayChart")} />
            <p className="muted small" style={{ marginBottom: 0 }}>
              {t("statistics.hoursOfDayHelp")}
            </p>
          </>
        ))}
    </Card>
  );
}

function Departures({ zone }: { zone: string | undefined }) {
  const [range, setRange] = useState(30);
  const to = dayOf(Date.now(), zone);
  const from = addDays(to, 1 - range);
  const tracks = useQuery({
    queryKey: ["listeners", "tracks", from, to],
    queryFn: () => unwrap(api.GET("/api/v1/listeners/tracks", { params: { query: { from, to, limit: 20 } } })),
  });
  return (
    <Card
      title={t("statistics.departures")}
      actions={
        <>
          <RangeSelect value={range} onChange={setRange} />
          <Csv href={csvUrl("tracks", { from, to, limit: 500 })} />
        </>
      }
      flush
    >
      <p className="muted small" style={{ margin: "0.75rem 1rem" }}>
        {t("statistics.departuresHelp")}
      </p>
      {tracks.isPending && <Loading />}
      {tracks.isError && <ErrorView error={tracks.error} />}
      {tracks.data && tracks.data.length === 0 && <Empty>{t("statistics.noTracks")}</Empty>}
      {tracks.data && tracks.data.length > 0 && (
        <div className="table-wrap">
          <table className="table">
            <thead>
              <tr>
                <th>{t("history.track")}</th>
                <th className="num">{t("statistics.plays")}</th>
                <th className="num hide-narrow">{t("statistics.atStart")}</th>
                <th className="num hide-narrow">{t("statistics.atEnd")}</th>
                <th className="num">{t("statistics.departed")}</th>
                <th className="num">{t("statistics.rate")}</th>
              </tr>
            </thead>
            <tbody>
              {tracks.data.map((track) => (
                <tr key={track.track_id}>
                  <td className="title-cell ellipsis">
                    <Link to={`/library/${track.track_id}`}>
                      {[track.artist, track.title].filter(Boolean).join(" – ") || t("history.unknownTrack")}
                    </Link>
                  </td>
                  <td className="num">{number(track.plays)}</td>
                  <td className="num hide-narrow">{decimal(track.listeners_start)}</td>
                  <td className="num hide-narrow">{decimal(track.listeners_end)}</td>
                  <td className="num">{track.departures == null ? "–" : number(track.departures)}</td>
                  <td className="num">{percent(track.departure_rate, locale())}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </Card>
  );
}

function Players({ zone, output }: { zone: string | undefined; output: number | undefined }) {
  const [range, setRange] = useState(30);
  const to = dayOf(Date.now(), zone);
  const from = addDays(to, 1 - range);
  const players = useQuery({
    queryKey: ["listeners", "players", from, to, output],
    queryFn: () =>
      unwrap(api.GET("/api/v1/listeners/players", { params: { query: { from, to, ...query({ output_id: output }) } } })),
  });
  return (
    <Card title={t("statistics.players")} actions={<RangeSelect value={range} onChange={setRange} />} flush>
      {players.isPending && <Loading />}
      {players.isError && <ErrorView error={players.error} />}
      {players.data && players.data.length === 0 && <Empty>{t("statistics.noSessions")}</Empty>}
      {players.data && players.data.length > 0 && (
        <div className="table-wrap">
          <table className="table">
            <thead>
              <tr>
                <th>{t("statistics.player")}</th>
                <th className="num">{t("statistics.sessions")}</th>
                <th className="num">{t("statistics.listeningHours")}</th>
              </tr>
            </thead>
            <tbody>
              {players.data.map((player) => (
                <tr key={player.user_agent}>
                  <td className="ellipsis small mono">
                    {player.user_agent || <span className="muted">{t("statistics.unknownPlayer")}</span>}
                  </td>
                  <td className="num">{number(player.sessions)}</td>
                  <td className="num">{decimal(player.listening_hours)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </Card>
  );
}
