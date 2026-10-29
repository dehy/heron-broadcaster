// SPDX-License-Identifier: AGPL-3.0-or-later

// The schedule: which clock runs at each hour of the week, in the
// station's time zone. Pick a clock, then paint hours, rectangles, days,
// an hour of every day or the week (docs/web.md › Painting the schedule).

import { useQuery } from "@tanstack/react-query";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { type ScheduledHour, api, unwrap } from "../api";
import { Card, Empty, ErrorView, Loading, PageHead } from "../components/ui";
import { rich, t } from "../i18n";
import { DAY_NUMBERS, dayName, hourLabel } from "../lib/rotation";
import {
  type Grid,
  type Hour,
  HOURS,
  WEEKDAYS,
  WEEKEND,
  cell,
  copyDay,
  everyDay,
  paint as paintHours,
  rectangle,
  wholeDays,
} from "../lib/schedule";
import { Link } from "../lib/router";
import { useAction, useCan } from "../lib/session";

const PALETTE = ["#2f6fd6", "#b4549a", "#1d7a3e", "#b07a00", "#0b6e66", "#6b4fbb", "#c62828", "#5f6268"];

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
  const [onlyEmpty, setOnlyEmpty] = useState(false);
  // The weeks before each stroke, to undo them.
  const [undo, setUndo] = useState<Grid[]>([]);
  // A drag: where it started, and the week before it.
  const stroke = useRef<{ start: Hour; base: Grid } | null>(null);
  // The hour clicked last, for Shift + click.
  const last = useRef<Hour | null>(null);
  const [copyFrom, setCopyFrom] = useState<number>(1);
  const [copyTo, setCopyTo] = useState<ReadonlySet<number>>(new Set());

  useEffect(() => {
    if (schedule.data) {
      setGrid(toGrid(schedule.data));
    }
  }, [schedule.data]);
  useEffect(() => {
    const stop = () => {
      stroke.current = null;
    };
    window.addEventListener("pointerup", stop);
    return () => window.removeEventListener("pointerup", stop);
  }, []);
  const undoLast = useCallback(() => {
    setUndo((stack) => {
      const previous = stack[stack.length - 1];
      if (previous) {
        setGrid(previous);
      }
      return stack.slice(0, -1);
    });
  }, []);
  useEffect(() => {
    const key = (event: KeyboardEvent) => {
      if ((event.ctrlKey || event.metaKey) && !event.shiftKey && event.key.toLowerCase() === "z") {
        const target = event.target as HTMLElement | null;
        if (target?.closest("input, textarea, select")) {
          return;
        }
        event.preventDefault();
        undoLast();
      }
    };
    window.addEventListener("keydown", key);
    return () => window.removeEventListener("keydown", key);
  }, [undoLast]);

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
  /** One stroke: these hours painted with the clock picked, undoable. */
  const apply = (hours: Hour[]) => {
    setUndo((stack) => [...stack, grid]);
    setGrid(paintHours(grid, hours, brush, onlyEmpty));
  };
  const brushName = brush === null ? t("schedule.noClockBrush") : (names.get(brush) ?? "?");
  const revert = () => {
    setGrid(toGrid(schedule.data));
    setUndo([]);
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
              <button
                type="button"
                className="btn"
                disabled={undo.length === 0}
                title={t("schedule.undoHelp")}
                onClick={undoLast}
              >
                {t("schedule.undo")}
              </button>
              {dirty && (
                <button type="button" className="btn" onClick={revert}>
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
            {canEdit && (
              <div className="row small">
                <button type="button" className="btn small" onClick={() => apply(wholeDays(WEEKDAYS))}>
                  {t("schedule.weekdays")}
                </button>
                <button type="button" className="btn small" onClick={() => apply(wholeDays(WEEKEND))}>
                  {t("schedule.weekend")}
                </button>
                <button type="button" className="btn small" onClick={() => apply(wholeDays(DAY_NUMBERS))}>
                  {t("schedule.wholeWeek")}
                </button>
                <label className="check">
                  <input type="checkbox" checked={onlyEmpty} onChange={(e) => setOnlyEmpty(e.target.checked)} />
                  {t("schedule.onlyEmpty")}
                </label>
                <span className="spacer" />
                <span className="row" role="group" aria-label={t("schedule.copyDay")}>
                  <span>{t("schedule.copy")}</span>
                  <select
                    className="input"
                    style={{ width: "auto" }}
                    aria-label={t("schedule.copyFrom")}
                    value={copyFrom}
                    onChange={(e) => setCopyFrom(Number(e.target.value))}
                  >
                      {DAY_NUMBERS.map((day) => (
                        <option key={day} value={day}>
                          {dayName(day)}
                        </option>
                      ))}
                  </select>
                  {t("schedule.onto")}
                  {DAY_NUMBERS.filter((day) => day !== copyFrom).map((day) => (
                    <label key={day} className="check">
                      <input
                        type="checkbox"
                        checked={copyTo.has(day)}
                        onChange={(e) =>
                          setCopyTo((current) => {
                            const next = new Set(current);
                            if (e.target.checked) {
                              next.add(day);
                            } else {
                              next.delete(day);
                            }
                            return next;
                          })
                        }
                      />
                      {dayName(day)}
                    </label>
                  ))}
                  <button
                    type="button"
                    className="btn small"
                    disabled={[...copyTo].every((day) => day === copyFrom)}
                    onClick={() => {
                      setUndo((stack) => [...stack, grid]);
                      setGrid(copyDay(grid, copyFrom, [...copyTo]));
                    }}
                  >
                    {t("schedule.copyButton")}
                  </button>
                </span>
              </div>
            )}
            <div className="table-wrap">
              <table className="schedule">
                <thead>
                  <tr>
                    <th />
                    {HOURS.map((hour) => {
                      const label = t("schedule.paintHour", { hour: hourLabel(hour), clock: brushName });
                      return (
                        <th key={hour} scope="col">
                          {canEdit ? (
                            <button
                              type="button"
                              className="head"
                              aria-label={label}
                              title={label}
                              onClick={() => apply(everyDay(hour))}
                            >
                              {String(hour).padStart(2, "0")}
                            </button>
                          ) : (
                            String(hour).padStart(2, "0")
                          )}
                        </th>
                      );
                    })}
                  </tr>
                </thead>
                <tbody>
                  {DAY_NUMBERS.map((day) => {
                    const name = dayName(day);
                    return (
                      <tr key={day}>
                        <th scope="row">
                          {canEdit ? (
                            <button
                              type="button"
                              className="head"
                              aria-label={t("schedule.paintDay", { day: name, clock: brushName })}
                              title={t("schedule.paintDay", { day: name, clock: brushName })}
                              onClick={() => apply(wholeDays([day]))}
                            >
                              {name}
                            </button>
                          ) : (
                            name
                          )}
                        </th>
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
                                  // The pointer may cross other cells: let them get its events.
                                  event.currentTarget.releasePointerCapture(event.pointerId);
                                  const here = { day, hour };
                                  const from = last.current;
                                  last.current = here;
                                  if (event.shiftKey && from) {
                                    apply(rectangle(from, here));
                                    return;
                                  }
                                  stroke.current = { start: here, base: grid };
                                  apply([here]);
                                }}
                                onPointerEnter={() => {
                                  const current = stroke.current;
                                  if (current) {
                                    setGrid(paintHours(current.base, rectangle(current.start, { day, hour }), brush, onlyEmpty));
                                  }
                                }}
                                onKeyDown={(event) => {
                                  if (event.key === "Enter" || event.key === " ") {
                                    event.preventDefault();
                                    const here = { day, hour };
                                    const from = last.current;
                                    last.current = here;
                                    apply(event.shiftKey && from ? rectangle(from, here) : [here]);
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
