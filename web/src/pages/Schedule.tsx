// SPDX-License-Identifier: AGPL-3.0-or-later

// The schedule: which clock runs at each hour of the week, in the
// station's time zone. Pick a clock, then click or drag over the hours.

import { useQuery } from "@tanstack/react-query";
import { useEffect, useMemo, useRef, useState } from "react";
import { type ScheduledHour, api, unwrap } from "../api";
import { Card, Empty, ErrorView, Loading, PageHead } from "../components/ui";
import { rich, t } from "../i18n";
import { DAY_NUMBERS, dayName, hourLabel } from "../lib/rotation";
import { Link } from "../lib/router";
import { useAction, useCan } from "../lib/session";

const PALETTE = ["#2f6fd6", "#b4549a", "#1d7a3e", "#b07a00", "#0b6e66", "#6b4fbb", "#c62828", "#5f6268"];

const HOURS = Array.from({ length: 24 }, (_, hour) => hour);

type Grid = Map<string, number>;

const cell = (day: number, hour: number) => `${day}-${hour}`;

function toGrid(hours: ScheduledHour[]): Grid {
  return new Map(hours.map((h) => [cell(h.day_of_week, h.hour), h.clock_id]));
}

function sameGrid(a: Grid, b: Grid): boolean {
  return a.size === b.size && [...a].every(([key, clock]) => b.get(key) === clock);
}

function fromGrid(grid: Grid): ScheduledHour[] {
  return [...grid.entries()].map(([key, clock_id]) => {
    const [day, hour] = key.split("-").map(Number);
    return { day_of_week: day ?? 1, hour: hour ?? 0, clock_id };
  });
}

export function Schedule() {
  const canEdit = useCan("operator");
  const clocks = useQuery({ queryKey: ["clocks"], queryFn: () => unwrap(api.GET("/api/v1/clocks")) });
  const schedule = useQuery({ queryKey: ["schedule"], queryFn: () => unwrap(api.GET("/api/v1/schedule")) });
  const station = useQuery({
    queryKey: ["settings", "station"],
    queryFn: () => unwrap(api.GET("/api/v1/settings/station")),
  });
  const [grid, setGrid] = useState<Grid>(new Map());
  // The clock painted; null paints "no clock", undefined until one is picked.
  const [picked, setBrush] = useState<number | null | undefined>(undefined);
  const brush = picked === undefined ? (clocks.data?.[0]?.id ?? null) : picked;
  const painting = useRef(false);

  useEffect(() => {
    if (schedule.data) {
      setGrid(toGrid(schedule.data));
    }
  }, [schedule.data]);
  useEffect(() => {
    const stop = () => {
      painting.current = false;
    };
    window.addEventListener("pointerup", stop);
    return () => window.removeEventListener("pointerup", stop);
  }, []);

  const colors = useMemo(
    () => new Map((clocks.data ?? []).map((clock, index) => [clock.id, PALETTE[index % PALETTE.length]])),
    [clocks.data],
  );
  const names = new Map((clocks.data ?? []).map((clock) => [clock.id, clock.name]));
  const save = useAction(() => unwrap(api.PUT("/api/v1/schedule", { body: fromGrid(grid) })), {
    success: t("schedule.saved"),
    invalidate: [["schedule"], ["clocks"]],
  });

  if (clocks.isPending || schedule.isPending) {
    return <Loading />;
  }
  if (clocks.isError || schedule.isError) {
    return <ErrorView error={clocks.error ?? schedule.error} />;
  }
  const dirty = !sameGrid(toGrid(schedule.data), grid);
  const paint = (day: number, hour: number) => {
    setGrid((current) => {
      const next = new Map(current);
      if (brush === null) {
        next.delete(cell(day, hour));
      } else {
        next.set(cell(day, hour), brush);
      }
      return next;
    });
  };
  const hoursOf = (clock: number) => [...grid.values()].filter((id) => id === clock).length;

  return (
    <>
      <PageHead
        title={t("nav.schedule")}
        description={rich("schedule.description", {
          link: <Link to="/clocks">{t("schedule.clock")}</Link>,
          zone: station.data?.timezone ?? "…",
        })}
        actions={
          canEdit && (
            <>
              {dirty && (
                <button type="button" className="btn" onClick={() => setGrid(toGrid(schedule.data))}>
                  {t("track.revert")}
                </button>
              )}
              <button
                type="button"
                className="btn primary"
                disabled={!dirty || save.isPending}
                onClick={() => save.mutate(undefined)}
              >
                {t("common.save")}
              </button>
            </>
          )
        }
      />
      {clocks.data.length === 0 ? (
        <Card>
          <Empty>
            {rich("schedule.noClock", { link: <Link to="/clocks/new">{t("schedule.createOne")}</Link> })}
          </Empty>
        </Card>
      ) : (
        <Card>
          <div className="stack">
            {canEdit && (
              <div className="row" role="radiogroup" aria-label={t("schedule.clockToSchedule")}>
                <span className="muted small">{t("schedule.paintWith")}</span>
                {clocks.data.map((clock) => (
                  <button
                    key={clock.id}
                    type="button"
                    role="radio"
                    aria-checked={brush === clock.id}
                    className={brush === clock.id ? "btn small brush active" : "btn small brush"}
                    onClick={() => setBrush(clock.id)}
                  >
                    <span className="swatch" style={{ background: colors.get(clock.id) }} />
                    {clock.name}
                    <span className="muted">{t("rotation.hours", { hours: hoursOf(clock.id) })}</span>
                  </button>
                ))}
                <button
                  type="button"
                  role="radio"
                  aria-checked={brush === null}
                  className={brush === null ? "btn small brush active" : "btn small brush"}
                  onClick={() => setBrush(null)}
                >
                  <span className="swatch empty-swatch" /> {t("schedule.noClockBrush")}
                </button>
              </div>
            )}
            <div className="table-wrap">
              <table
                className="schedule"
                onPointerLeave={() => {
                  painting.current = false;
                }}
              >
                <thead>
                  <tr>
                    <th />
                    {HOURS.map((hour) => (
                      <th key={hour} scope="col" title={hourLabel(hour)}>
                        {String(hour).padStart(2, "0")}
                      </th>
                    ))}
                  </tr>
                </thead>
                <tbody>
                  {DAY_NUMBERS.map((day) => {
                    const name = dayName(day);
                    return (
                      <tr key={day}>
                        <th scope="row">{name}</th>
                        {HOURS.map((hour) => {
                          const clock = grid.get(cell(day, hour));
                          const label = t("schedule.cell", {
                            day: name,
                            hour: hourLabel(hour),
                            clock: clock !== undefined ? (names.get(clock) ?? "?") : t("schedule.noClockCell"),
                          });
                          return (
                            <td key={hour}>
                              <button
                                type="button"
                                className="hour"
                                aria-label={label}
                                title={label}
                                disabled={!canEdit}
                                style={{ background: clock !== undefined ? colors.get(clock) : undefined }}
                                onPointerDown={(event) => {
                                  event.preventDefault();
                                  painting.current = true;
                                  paint(day, hour);
                                }}
                                onPointerEnter={() => {
                                  if (painting.current) {
                                    paint(day, hour);
                                  }
                                }}
                                onKeyDown={(event) => {
                                  if (event.key === "Enter" || event.key === " ") {
                                    event.preventDefault();
                                    paint(day, hour);
                                  }
                                }}
                              />
                            </td>
                          );
                        })}
                      </tr>
                    );
                  })}
                </tbody>
              </table>
            </div>
            <div className="row small muted">
              {clocks.data.map((clock) => (
                <span key={clock.id} className="row" style={{ gap: "0.3rem" }}>
                  <span className="swatch" style={{ background: colors.get(clock.id) }} />
                  {rich("schedule.clockHours", {
                    clock: <Link to={`/clocks/${clock.id}`}>{clock.name}</Link>,
                    count: hoursOf(clock.id),
                  })}
                </span>
              ))}
              <span>{t("schedule.unscheduled", { count: 168 - grid.size })}</span>
            </div>
          </div>
        </Card>
      )}
    </>
  );
}
