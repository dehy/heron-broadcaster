// SPDX-License-Identifier: AGPL-3.0-or-later

// Library management through heron-librarian: identifications to review,
// organizing the library after a dry run, and undoing batches.

import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useState } from "react";
import { type LibraryTask, type Schemas, api, done, unwrap } from "../api";
import {
  Banner,
  Card,
  ConfirmButton,
  Empty,
  ErrorView,
  Icon,
  Loading,
  PageHead,
  useToast,
} from "../components/ui";
import { ago, count, dateTime, duration, trackName } from "../lib/format";
import { Link, setSearch, useLocation } from "../lib/router";
import { useAction, useCan } from "../lib/session";

type Tab = "review" | "organize" | "batches";

/** A library task, polled until heron-librarian finishes it. */
function useTask(id: number | null) {
  return useQuery({
    queryKey: ["library-task", id],
    enabled: id !== null,
    queryFn: () => unwrap(api.GET("/api/v1/library/tasks/{id}", { params: { path: { id: id ?? 0 } } })),
    refetchInterval: (query) => {
      const status = query.state.data?.status;
      return status === "done" || status === "failed" ? false : 1000;
    },
  });
}

function TaskState({ task }: { task: LibraryTask | undefined }) {
  if (!task) {
    return null;
  }
  switch (task.status) {
    case "pending":
      return (
        <span className="row small muted">
          <span className="spinner" /> Waiting for heron-librarian…
        </span>
      );
    case "running":
      return (
        <span className="row small muted">
          <span className="spinner" /> Working…
        </span>
      );
    case "failed":
      return <span className="small bad">Failed: {task.error}</span>;
    case "done":
      return <span className="small good">Done {ago(task.finished_at_unix_ms)}</span>;
  }
}

export function Review() {
  const { search } = useLocation();
  const tab = (search.get("tab") ?? "review") as Tab;
  return (
    <>
      <PageHead
        title="Review & organize"
        description="heron-librarian, the only service that writes to the library, carries these out; every change can be undone."
      />
      <nav className="tabs" aria-label="Sections">
        {(
          [
            ["review", "Identifications"],
            ["organize", "Organize"],
            ["batches", "Changes"],
          ] as const
        ).map(([value, label]) => (
          <button
            key={value}
            type="button"
            aria-selected={tab === value}
            onClick={() => setSearch({ tab: value === "review" ? undefined : value })}
          >
            {label}
          </button>
        ))}
      </nav>
      {tab === "review" && <Identifications />}
      {tab === "organize" && <Organize />}
      {tab === "batches" && <Batches />}
    </>
  );
}

// Identifications -----------------------------------------------------------

function Identifications() {
  const review = useQuery({
    queryKey: ["review"],
    queryFn: () => unwrap(api.GET("/api/v1/library/review", { params: { query: { limit: 50 } } })),
  });
  if (review.isPending) {
    return <Loading />;
  }
  if (review.isError) {
    return <ErrorView error={review.error} />;
  }
  if (review.data.items.length === 0) {
    return (
      <Card>
        <Empty>
          Nothing to review. Files dropped into the incoming folder are identified with AcoustID and
          MusicBrainz; the doubtful matches wait here.
        </Empty>
      </Card>
    );
  }
  return (
    <div className="stack">
      <p className="muted" style={{ margin: 0 }}>
        {count(review.data.total, "track")} imported with their own tags: their identification was
        not confident enough. Pick the right release, or keep their tags.
      </p>
      {review.data.items.map((item) => (
        <ReviewItem key={item.track.id} item={item} />
      ))}
    </div>
  );
}

function ReviewItem({ item }: { item: Schemas["ReviewItem"] }) {
  const canEdit = useCan("operator");
  const client = useQueryClient();
  const toast = useToast();
  const [taskId, setTaskId] = useState<number | null>(null);
  const task = useTask(taskId);
  const apply = useAction(
    (candidate: Schemas["ReviewCandidate"]) =>
      unwrap(
        api.POST("/api/v1/tracks/{id}/review/apply", {
          params: { path: { id: item.track.id } },
          body: { release_id: candidate.release_id, recording_id: candidate.recording_id },
        }),
      ),
    { onSuccess: (queued) => setTaskId(queued.id) },
  );
  const keep = useAction(
    () => done(api.POST("/api/v1/tracks/{id}/review/keep", { params: { path: { id: item.track.id } } })),
    { success: "Kept its own tags", invalidate: [["review"], ["review-count"]] },
  );

  useEffect(() => {
    if (task.data?.status === "done") {
      toast(`${trackName(item.track)}: release applied`);
      void client.invalidateQueries({ queryKey: ["review"] });
      void client.invalidateQueries({ queryKey: ["review-count"] });
      void client.invalidateQueries({ queryKey: ["library-batches"] });
    } else if (task.data?.status === "failed") {
      toast(task.data.error ?? "The release could not be applied", true);
    }
  }, [task.data?.status, task.data?.error, client, toast, item.track]);

  const busy = taskId !== null && task.data?.status !== "failed";
  return (
    <Card
      title={
        <div style={{ minWidth: 0 }}>
          <h2 className="ellipsis">
            <Link to={`/library/${item.track.id}`}>{trackName(item.track)}</Link>
          </h2>
          <div className="muted small ellipsis">
            <code>{item.track.path}</code> · {duration(item.track.duration_ms)}
          </div>
        </div>
      }
      actions={
        canEdit && (
          <button type="button" className="btn small" disabled={busy} onClick={() => keep.mutate(undefined)}>
            <Icon name="check" size={14} /> Keep its tags
          </button>
        )
      }
      flush
    >
      <div className="card-body row between">
        <span className="warn small">{item.reason ?? "Needs review"}</span>
        <TaskState task={task.data} />
      </div>
      {item.candidates.length === 0 ? (
        <Empty>No candidate release: keep its tags, or fix them and drop the file again.</Empty>
      ) : (
        <div className="table-wrap">
          <table className="table">
            <thead>
              <tr>
                <th>Release</th>
                <th className="hide-narrow">Date</th>
                <th className="hide-narrow">Country</th>
                <th>Track</th>
                <th>Score</th>
                <th>
                  <span className="visually-hidden">Actions</span>
                </th>
              </tr>
            </thead>
            <tbody>
              {item.candidates.map((candidate) => (
                <tr key={`${candidate.release_id}-${candidate.recording_id}-${candidate.disc}-${candidate.track}`}>
                  <td className="title-cell">
                    <a
                      href={`https://musicbrainz.org/release/${candidate.release_id}`}
                      target="_blank"
                      rel="noreferrer"
                    >
                      {candidate.release_title ?? candidate.release_group_title ?? candidate.release_id}
                    </a>
                  </td>
                  <td className="hide-narrow">{candidate.date ?? "–"}</td>
                  <td className="hide-narrow">{candidate.country ?? "–"}</td>
                  <td className="num">
                    {candidate.disc && candidate.disc > 1 ? `${candidate.disc}-` : ""}
                    {candidate.track ?? "–"}
                  </td>
                  <td className="num">{Math.round(candidate.score * 100)}%</td>
                  <td className="actions">
                    {canEdit && (
                      <button
                        type="button"
                        className="btn small primary"
                        disabled={busy || apply.isPending}
                        onClick={() => apply.mutate(candidate)}
                      >
                        Apply
                      </button>
                    )}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </Card>
  );
}

// Organize ------------------------------------------------------------------

function Organize() {
  const isAdmin = useCan("admin");
  const client = useQueryClient();
  const settings = useQuery({
    queryKey: ["settings", "library"],
    queryFn: () => unwrap(api.GET("/api/v1/settings/library")),
  });
  const recent = useQuery({
    queryKey: ["library-tasks"],
    queryFn: () => unwrap(api.GET("/api/v1/library/tasks", { params: { query: { limit: 50 } } })),
  });
  const [planId, setPlanId] = useState<number | null>(null);
  const [applyId, setApplyId] = useState<number | null>(null);

  // Pick up the last dry run (and its apply) after a reload.
  useEffect(() => {
    if (planId !== null || !recent.data) {
      return;
    }
    const plan = recent.data.find((task) => task.request.kind === "organize_plan");
    if (plan) {
      setPlanId(plan.id);
      const applied = recent.data.find(
        (task) => task.request.kind === "organize_apply" && task.request.plan_task_id === plan.id,
      );
      if (applied) {
        setApplyId(applied.id);
      }
    }
  }, [recent.data, planId]);

  const plan = useTask(planId);
  const applied = useTask(applyId);
  const dryRun = useAction(() => unwrap(api.POST("/api/v1/library/organize")), {
    onSuccess: (task) => {
      setPlanId(task.id);
      setApplyId(null);
    },
  });
  const apply = useAction(
    (id: number) =>
      unwrap(api.POST("/api/v1/library/organize/{plan_task_id}/apply", { params: { path: { plan_task_id: id } } })),
    { onSuccess: (task) => setApplyId(task.id) },
  );
  useEffect(() => {
    if (applied.data?.status === "done") {
      void client.invalidateQueries({ queryKey: ["library-batches"] });
      void client.invalidateQueries({ queryKey: ["tracks"] });
    }
  }, [applied.data?.status, client]);

  const moves = plan.data?.plan?.moves ?? [];
  const [shown, setShown] = useState(100);

  return (
    <div className="stack">
      <Card
        title="Organize the library"
        actions={
          isAdmin && (
            <button
              type="button"
              className="btn primary"
              disabled={dryRun.isPending || plan.data?.status === "pending" || plan.data?.status === "running"}
              onClick={() => dryRun.mutate(undefined)}
            >
              {plan.data ? "New dry run" : "Dry run"}
            </button>
          )
        }
      >
        <div className="stack">
          <p style={{ margin: 0 }}>
            Moves every track where the layout puts it, using the files' current tags:{" "}
            <code>{settings.data?.layout ?? "…"}</code> (<Link to="/settings">change it</Link>).
          </p>
          <p className="muted small" style={{ margin: 0 }}>
            The dry run lists every move and changes nothing. Applying it moves the files as one batch,
            which can be undone. Files are never overwritten: a name already taken gets “ (2)”.
          </p>
          {!isAdmin && <p className="muted small">Only admins can organize the library.</p>}
        </div>
      </Card>

      {plan.data && (
        <Card
          title={`Dry run ${dateTime(plan.data.created_at_unix_ms)}`}
          actions={
            <>
              <TaskState task={plan.data} />
              {isAdmin && plan.data.status === "done" && moves.length > 0 && !applyId && (
                <ConfirmButton
                  className="btn primary"
                  confirm={`Move ${count(moves.length, "file")}`}
                  question={
                    <p>
                      Move {count(moves.length, "file")} as listed? Tracks moved since the dry run are skipped.
                      The batch can be undone from Changes.
                    </p>
                  }
                  onConfirm={() => apply.mutate(plan.data?.id ?? 0)}
                >
                  Apply {count(moves.length, "move")}
                </ConfirmButton>
              )}
            </>
          }
          flush
        >
          {applied.data && (
            <div className="card-body stack">
              <div className="row between">
                <strong>Applying</strong>
                <TaskState task={applied.data} />
              </div>
              {applied.data.report && (
                <BatchReport report={applied.data.report} />
              )}
            </div>
          )}
          {plan.data.plan && (
            <>
              <div className="card-body">
                {count(moves.length, "move")}, {count(plan.data.plan.unchanged, "track")} already in place
                {plan.data.plan.unreadable.length > 0 &&
                  `, ${plan.data.plan.unreadable.length} unreadable (left where they are)`}
                .
              </div>
              {moves.length > 0 && (
                <div className="table-wrap">
                  <table className="table">
                    <thead>
                      <tr>
                        <th>From</th>
                        <th>To</th>
                      </tr>
                    </thead>
                    <tbody>
                      {moves.slice(0, shown).map((move) => (
                        <tr key={move.track_id}>
                          <td className="mono small" style={{ overflowWrap: "anywhere" }}>
                            {move.from}
                          </td>
                          <td className="mono small" style={{ overflowWrap: "anywhere" }}>
                            {move.to}
                          </td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </div>
              )}
              {moves.length > shown && (
                <div className="pager">
                  <span className="muted small">{moves.length - shown} more</span>
                  <button type="button" className="btn small" onClick={() => setShown((n) => n + 500)}>
                    Show more
                  </button>
                </div>
              )}
            </>
          )}
        </Card>
      )}
    </div>
  );
}

function BatchReport({ report }: { report: Schemas["BatchReport"] }) {
  return (
    <div className="stack">
      <span>
        Batch {report.batch_id}: {count(report.done, "file")} done
        {report.skipped.length > 0 && `, ${report.skipped.length} skipped`}.
      </span>
      {report.skipped.length > 0 && (
        <ul className="small" style={{ margin: 0 }}>
          {report.skipped.slice(0, 50).map((skipped) => (
            <li key={skipped.path}>
              <code>{skipped.path}</code>: {skipped.reason}
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

// Batches -------------------------------------------------------------------

const KINDS: Record<string, string> = {
  import: "Import",
  organize: "Organization",
  undo: "Undo",
  review: "Review",
};

function Batches() {
  const isAdmin = useCan("admin");
  const client = useQueryClient();
  const toast = useToast();
  const batches = useQuery({
    queryKey: ["library-batches"],
    queryFn: () => unwrap(api.GET("/api/v1/library/batches", { params: { query: { limit: 50 } } })),
  });
  const [undoId, setUndoId] = useState<number | null>(null);
  const undoing = useTask(undoId);
  const undo = useAction(
    (id: number) => unwrap(api.POST("/api/v1/library/batches/{id}/undo", { params: { path: { id } } })),
    { onSuccess: (task) => setUndoId(task.id) },
  );
  useEffect(() => {
    const status = undoing.data?.status;
    if (status === "done") {
      const report = undoing.data?.report;
      toast(
        report
          ? `Undone: ${count(report.done, "file")} back${report.skipped.length ? `, ${report.skipped.length} left in place` : ""}`
          : "Undone",
      );
      void client.invalidateQueries({ queryKey: ["library-batches"] });
      void client.invalidateQueries({ queryKey: ["review"] });
    } else if (status === "failed") {
      toast(undoing.data?.error ?? "The undo failed", true);
    }
  }, [undoing.data?.status, undoing.data?.report, undoing.data?.error, client, toast]);

  if (batches.isPending) {
    return <Loading />;
  }
  if (batches.isError) {
    return <ErrorView error={batches.error} />;
  }
  const busy = undoId !== null && undoing.data?.status !== "done" && undoing.data?.status !== "failed";
  return (
    <div className="stack">
      {busy && (
        <Banner kind="warn">
          Undoing… <TaskState task={undoing.data} />
        </Banner>
      )}
      <Card flush>
        {batches.data.length === 0 ? (
          <Empty>No change yet.</Empty>
        ) : (
          <table className="table">
            <thead>
              <tr>
                <th>Batch</th>
                <th>When</th>
                <th>Files</th>
                <th>State</th>
                <th>
                  <span className="visually-hidden">Actions</span>
                </th>
              </tr>
            </thead>
            <tbody>
              {batches.data.map((batch) => (
                <tr key={batch.id}>
                  <td>
                    #{batch.id} {KINDS[batch.kind]}
                    {batch.undoes && <span className="muted"> of #{batch.undoes}</span>}
                  </td>
                  <td>{dateTime(batch.created_at_unix_ms)}</td>
                  <td className="num">{batch.files}</td>
                  <td>
                    {batch.undone_by ? (
                      <span className="badge">Undone by #{batch.undone_by}</span>
                    ) : batch.finished_at_unix_ms ? (
                      <span className="badge good">Done</span>
                    ) : (
                      <span className="badge warn">Unfinished</span>
                    )}
                  </td>
                  <td className="actions">
                    {isAdmin && batch.kind !== "undo" && !batch.undone_by && batch.files > 0 && (
                      <ConfirmButton
                        className="btn small"
                        confirm="Undo"
                        disabled={busy}
                        question={
                          <p>
                            Put the {count(batch.files, "file")} of batch #{batch.id} back where they were, with
                            their previous tags? Files whose previous path is taken stay where they are.
                          </p>
                        }
                        onConfirm={() => undo.mutate(batch.id)}
                      >
                        <Icon name="undo" size={14} /> Undo
                      </ConfirmButton>
                    )}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </Card>
    </div>
  );
}
