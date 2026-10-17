// SPDX-License-Identifier: AGPL-3.0-or-later

// The selection log: every track rotation queued, and why. And simulations
// of the programming to come, checked rule by rule.

import { keepPreviousData, useQuery } from "@tanstack/react-query";
import { Fragment, useState } from "react";
import { type Explanation, type RotationPick, type SimulationReport, api, query, unwrap } from "../api";
import { Banner, Card, Empty, ErrorView, Field, Loading, PageHead, Segmented } from "../components/ui";
import { ago, count, dateTime } from "../lib/format";
import { describeWindows, methodLabel } from "../lib/rotation";
import { Link } from "../lib/router";
import { useAction, useCan } from "../lib/session";
import { useCategories } from "./Clocks";

export function Rotation() {
  const canSimulate = useCan("operator");
  return (
    <>
      <PageHead
        title="Selection log"
        description="Why each track was picked: the clock and slot, the rules held or relaxed, how many tracks were left to choose from."
      />
      {canSimulate && <SimulationCard />}
      <PicksCard />
    </>
  );
}

function StepBadge({ explanation }: { explanation: Explanation }) {
  if (!explanation.enforce_rules) {
    return <span className="badge">Rules off</span>;
  }
  if (explanation.step === 0) {
    return <span className="badge good">Rules held</span>;
  }
  return (
    <span className="badge warn" title={explanation.step_description}>
      Relaxed: {explanation.step_description}
    </span>
  );
}

function Where({ explanation }: { explanation: Explanation }) {
  const categories = useCategories();
  const name = (id: number | null | undefined) =>
    id === null || id === undefined ? undefined : categories.data?.find((c) => c.id === id)?.name;
  const slotCategory = name(explanation.slot_category_id) ?? explanation.category_name ?? "the whole library";
  if (explanation.clock_id === null || explanation.clock_id === undefined) {
    return <span className="muted">No clock: the whole library</span>;
  }
  return (
    <span>
      <Link to={`/clocks/${explanation.clock_id}`}>{explanation.clock_name ?? "Clock"}</Link>, slot{" "}
      {(explanation.slot ?? 0) + 1}: {slotCategory}
      {explanation.step >= 9 && <span className="muted"> → {explanation.category_name ?? "the whole library"}</span>}
    </span>
  );
}

function Details({ explanation }: { explanation: Explanation }) {
  const blocked = explanation.blocked;
  const held = [
    blocked.track && count(blocked.track, "track") + " played too recently",
    blocked.title && count(blocked.title, "title") + " heard too recently",
    blocked.artist && count(blocked.artist, "artist") + " heard too recently",
    blocked.album && count(blocked.album, "album") + " heard too recently",
  ].filter(Boolean);
  return (
    <dl className="facts small">
      <dt>Method</dt>
      <dd>{methodLabel(explanation.method)}</dd>
      <dt>Category</dt>
      <dd>
        {count(explanation.pool, "track")}
        {explanation.restricted > 0 && `, ${explanation.restricted} forbidden by restrictions`}
      </dd>
      {held.length > 0 && (
        <>
          <dt>Held back</dt>
          <dd>{held.join(", ")}</dd>
        </>
      )}
      <dt>Chosen among</dt>
      <dd>{count(explanation.candidates, "track")}</dd>
      <dt>Windows applied</dt>
      <dd>{describeWindows(explanation.windows)}</dd>
      <dt>Last played</dt>
      <dd>
        {explanation.last_played_unix_ms ? `${ago(explanation.last_played_unix_ms)}` : "never"}
        {explanation.method === "weighted" && ` · weight ${explanation.weight.toFixed(2)}`}
      </dd>
      {explanation.skipped_slots.length > 0 && (
        <>
          <dt>Skipped</dt>
          <dd>
            {explanation.skipped_slots.map((slot) => `slot ${slot.slot + 1}`).join(", ")}: restrictions left nothing
          </dd>
        </>
      )}
    </dl>
  );
}

function PicksCard() {
  const [before, setBefore] = useState<number[]>([]);
  const [open, setOpen] = useState<number | null>(null);
  const cursor = before[before.length - 1];
  const picks = useQuery({
    queryKey: ["rotation-picks", cursor],
    queryFn: () =>
      unwrap(api.GET("/api/v1/rotation/picks", { params: { query: query({ before: cursor, limit: 50 }) } })),
    placeholderData: keepPreviousData,
    refetchInterval: cursor === undefined ? 15_000 : false,
  });
  const rows: RotationPick[] = picks.data ?? [];
  const last = rows[rows.length - 1];
  return (
    <Card title="Picks" flush>
      {picks.isPending && <Loading />}
      {picks.isError && <ErrorView error={picks.error} />}
      {picks.data?.length === 0 && <Empty>Nothing picked yet: autofill picks as the queue runs low.</Empty>}
      {rows.length > 0 && (
        <div className="table-wrap">
          <table className="table">
            <thead>
              <tr>
                <th>Planned</th>
                <th>Track</th>
                <th>From</th>
                <th>Rules</th>
              </tr>
            </thead>
            <tbody>
              {rows.map((pick) => (
                <Fragment key={pick.id}>
                  <tr onClick={() => setOpen(open === pick.id ? null : pick.id)} style={{ cursor: "pointer" }}>
                    <td className="num small">
                      {dateTime(pick.planned_at_unix_ms)}
                      {!pick.played_at_unix_ms && <div className="muted">not played yet</div>}
                    </td>
                    <td className="title-cell">
                      {pick.track_id ? (
                        <Link to={`/library/${pick.track_id}`} onClick={(event) => event.stopPropagation()}>
                          {pick.artist ?? "?"} – {pick.title ?? "?"}
                        </Link>
                      ) : (
                        <span className="muted">deleted track</span>
                      )}
                    </td>
                    <td className="small">
                      <Where explanation={pick.explanation} />
                    </td>
                    <td>
                      <StepBadge explanation={pick.explanation} />
                    </td>
                  </tr>
                  {open === pick.id && (
                    <tr>
                      <td colSpan={4}>
                        <Details explanation={pick.explanation} />
                      </td>
                    </tr>
                  )}
                </Fragment>
              ))}
            </tbody>
          </table>
        </div>
      )}
      {(before.length > 0 || rows.length === 50) && (
        <div className="pager">
          <button
            type="button"
            className="btn small"
            disabled={before.length === 0}
            onClick={() => setBefore(before.slice(0, -1))}
          >
            Newer
          </button>
          <button
            type="button"
            className="btn small"
            disabled={rows.length < 50 || !last}
            onClick={() => last && setBefore([...before, last.id])}
          >
            Older
          </button>
        </div>
      )}
    </Card>
  );
}

type Span = "24" | "168";

function SimulationCard() {
  const [hours, setHours] = useState<Span>("168");
  const [report, setReport] = useState<SimulationReport | null>(null);
  const simulate = useAction(
    () => unwrap(api.POST("/api/v1/rotation/simulate", { body: { hours: Number(hours) } })),
    { onSuccess: setReport },
  );
  return (
    <Card
      title="Simulation"
      actions={
        <>
          <Segmented
            label="Length"
            value={hours}
            options={[
              { value: "24", label: "A day" },
              { value: "168", label: "A week" },
            ]}
            onChange={setHours}
          />
          <button
            type="button"
            className="btn small primary"
            disabled={simulate.isPending}
            onClick={() => simulate.mutate(undefined)}
          >
            {simulate.isPending ? "Simulating…" : "Simulate"}
          </button>
        </>
      }
    >
      {!report ? (
        <p className="muted small" style={{ margin: 0 }}>
          Generates the programming to come from the clocks, the schedule and the library, without touching the
          queue, then checks every pick: separation as relaxed at its step, restrictions always.
        </p>
      ) : (
        <SimulationSummary report={report} />
      )}
    </Card>
  );
}

function SimulationSummary({ report }: { report: SimulationReport }) {
  const simulation = report.simulation;
  const [shown, setShown] = useState(100);
  const names = new Map(report.tracks.map((track) => [track.id, track]));
  const picks = simulation.picks.length;
  const relaxed = simulation.steps.slice(1).reduce((sum, n) => sum + n, 0);
  const off = simulation.picks.filter((pick) => !pick.explanation.enforce_rules).length;
  return (
    <div className="stack">
      <div className="grid three">
        <div className="stat">
          <span className="value">{picks.toLocaleString()}</span>
          <span className="label">tracks picked</span>
        </div>
        <div className="stat">
          <span className="value">{simulation.violations.length === 0 ? "None" : simulation.violations.length}</span>
          <span className="label">rules broken</span>
        </div>
        <div className="stat">
          <span className="value">{picks ? `${((relaxed / picks) * 100).toFixed(1)} %` : "–"}</span>
          <span className="label">logged relaxations</span>
        </div>
      </div>
      {simulation.violations.length > 0 && (
        <Banner kind="bad">
          {count(simulation.violations.length, "pick")} broke a rule. This should not happen: please report it with
          seed {report.seed}.
        </Banner>
      )}
      {simulation.gap_minutes > 0 && (
        <Banner kind="warn">
          For {count(simulation.gap_minutes, "minute")}, restrictions forbade every track: the engine's fallback
          directory would play.
        </Banner>
      )}
      <Field label="Relaxation steps">
        <ul className="steps">
          {simulation.steps.map((n, step) =>
            n === 0 ? null : (
              <li key={step}>
                <span className="num">{step}</span>
                <span className="bar" style={{ width: `${Math.max(1, (n / picks) * 100)}%` }} />
                <span className="small muted">
                  {n.toLocaleString()} {step === 0 ? "with every rule" : ""}
                </span>
              </li>
            ),
          )}
        </ul>
      </Field>
      {off > 0 && <p className="muted small">{count(off, "pick")} from slots that do not enforce the rules.</p>}
      <div className="table-wrap">
        <table className="table">
          <thead>
            <tr>
              <th>Time</th>
              <th>Track</th>
              <th>From</th>
              <th>Rules</th>
            </tr>
          </thead>
          <tbody>
            {simulation.picks.slice(0, shown).map((pick, index) => {
              const track = names.get(pick.track_id);
              return (
                <tr key={index}>
                  <td className="num small">{dateTime(pick.start_unix_ms)}</td>
                  <td className="title-cell">
                    <Link to={`/library/${pick.track_id}`}>
                      {track ? `${track.artist ?? "?"} – ${track.title ?? "?"}` : `Track ${pick.track_id}`}
                    </Link>
                  </td>
                  <td className="small">
                    <Where explanation={pick.explanation} />
                  </td>
                  <td>
                    <StepBadge explanation={pick.explanation} />
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>
      {shown < picks && (
        <div className="pager">
          <span className="muted small">
            {shown} of {picks.toLocaleString()}
          </span>
          <button type="button" className="btn small" onClick={() => setShown(shown + 200)}>
            Show more
          </button>
        </div>
      )}
    </div>
  );
}
