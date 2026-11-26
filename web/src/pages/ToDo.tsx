// SPDX-License-Identifier: AGPL-3.0-or-later

// What is left to do on the library (docs/web.md › To do): identifications
// to review, the songs missing an intro, a hook or a cover, and, through
// heron-librarian, organizing the library after a dry run and undoing batches.

import { ModuleOff } from "../components/Modules";
import { useModule } from "../lib/modules";
import { CoverCandidates } from "../components/Cover";
import { keepPreviousData, useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useState } from "react";
import { type Schemas, api, done, unwrap } from "../api";
import { TaskState, useTask } from "../components/Task";
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
import { type PlainKey, number, rich, t } from "../i18n";
import { dateTime, duration, trackName } from "../lib/format";
import { Link, setSearch, useLocation } from "../lib/router";
import { useAction, useCan } from "../lib/session";
import { LibraryReviewDialog } from "./LibraryReview";

type Tab = "review" | "damaged" | "intros" | "hooks" | "covers" | "organize" | "batches";

/** The query key of the tabs' counts, refreshed with what changes them. */
export const TODO_COUNTS = ["todo-counts"];

export function ToDo() {
  const { search } = useLocation();
  const tab = (search.get("tab") ?? "review") as Tab;
  const coverArt = useModule("cover_art");
  const counts = useQuery({
    queryKey: TODO_COUNTS,
    queryFn: () => unwrap(api.GET("/api/v1/library/todo")),
  });
  const count: Partial<Record<Tab, number | null | undefined>> = {
    review: counts.data?.to_identify,
    damaged: counts.data?.damaged,
    intros: counts.data?.no_intro,
    hooks: counts.data?.no_hook,
    covers: counts.data?.no_cover,
  };
  return (
    <>
      <PageHead
        title={t("nav.review")}
        description={t("review.description")}
      />
      <nav className="tabs" aria-label={t("review.sections")}>
        {(
          [
            ["review", "todo.tab.identify"],
            ["damaged", "todo.tab.damaged"],
            ["intros", "todo.tab.intros"],
            ["hooks", "todo.tab.hooks"],
            ["covers", "todo.tab.covers"],
            ["organize", "review.tab.organize"],
            ["batches", "review.tab.changes"],
          ] as const satisfies readonly (readonly [Tab, PlainKey])[]
        )
          .filter(([value]) => coverArt || value !== "covers")
          .map(([value, label]) => {
            const n = count[value];
            return (
              <button
                key={value}
                type="button"
                aria-current={tab === value ? "page" : undefined}
                onClick={() =>
                  setSearch({ tab: value === "review" ? undefined : value, page: undefined, sort: undefined, desc: undefined })
                }
              >
                {t(label)} {n ? <span className="count">{number(n)}</span> : null}
              </button>
            );
          })}
      </nav>
      {tab === "review" && <Identifications />}
      {tab === "damaged" && <DamagedTracks />}
      {tab === "intros" && <MissingSongs filter="no_intro" empty="todo.noMissingIntro" />}
      {tab === "hooks" && <MissingSongs filter="no_hook" empty="todo.noMissingHook" />}
      {tab === "covers" &&
        (coverArt ? (
          <div className="stack">
            <CoversToCheck />
            <MissingSongs filter="no_cover" empty="todo.noMissingCover" trackTab="details" />
          </div>
        ) : (
          <ModuleOff module="cover_art" />
        ))}
      {tab === "organize" && <Organize />}
      {tab === "batches" && <Batches />}
    </>
  );
}

// Songs missing an intro, a hook or a cover ---------------------------------

const PAGE = 50;

/** The built-in Song category type (`SONG_TYPE_ID` in the API). */
const SONG_TYPE_ID = 1;

type MissingSort = "artist" | "title";

function MissingSongs({
  filter,
  empty,
  trackTab,
}: Readonly<{
  filter: "no_intro" | "no_hook" | "no_cover";
  empty: PlainKey;
  /** The tab of the track's page that fixes it: *Sound* by default. */
  trackTab?: "details";
}>) {
  const { search } = useLocation();
  const page = Math.max(0, Number(search.get("page") ?? 0) || 0);
  const sort: MissingSort = search.get("sort") === "title" ? "title" : "artist";
  const desc = search.get("desc") === "true";
  const category = Number(search.get("category")) || undefined;
  const categories = useQuery({
    queryKey: ["categories", "all"],
    queryFn: () => unwrap(api.GET("/api/v1/categories")),
  });
  const songCategories = (categories.data ?? []).filter((c) => c.type_id === SONG_TYPE_ID);
  const songs = useQuery({
    queryKey: ["tracks", "todo", filter, sort, desc, page, category],
    queryFn: () =>
      unwrap(
        api.GET("/api/v1/tracks", {
          params: {
            query: {
              active_songs: true,
              filter,
              sort,
              desc,
              limit: PAGE,
              offset: page * PAGE,
              ...(category !== undefined && { category_id: category }),
            },
          },
        }),
      ),
    placeholderData: keepPreviousData,
  });
  const items = songs.data?.items ?? [];
  const total = songs.data?.total ?? 0;
  const pages = Math.max(1, Math.ceil(total / PAGE));
  const href = (id: number) => (trackTab ? `/library/${id}?tab=${trackTab}` : `/library/${id}`);
  return (
    <Card flush>
      <div className="card-body row between">
        <span className="muted small">{t("todo.songsHelp")}</span>
        <span className="row" style={{ flexWrap: "nowrap", alignItems: "flex-end" }}>
          <label className="stacked">
            <span>{t("library.categories")}</span>
            <select
              className="input"
              value={category ?? ""}
              onChange={(event) => setSearch({ category: event.target.value || undefined, page: undefined })}
            >
              <option value="">{t("library.anyCategory")}</option>
              {songCategories.map((c) => (
                <option key={c.id} value={c.id}>
                  {c.name}
                </option>
              ))}
            </select>
          </label>
          <label className="stacked">
            <span>{t("library.sortLabel")}</span>
            <select
              className="input"
              value={sort}
              onChange={(event) =>
                setSearch({ sort: event.target.value === "artist" ? undefined : event.target.value, page: undefined })
              }
            >
              <option value="artist">{t("library.sort.artist")}</option>
              <option value="title">{t("library.sort.title")}</option>
            </select>
          </label>
          <button
            type="button"
            className="btn ghost small"
            aria-pressed={desc}
            title={t("library.reverseHelp")}
            onClick={() => setSearch({ desc: desc ? undefined : "true", page: undefined })}
          >
            {desc ? t("library.reversed") : t("library.reverse")}
          </button>
        </span>
      </div>
      {songs.isPending && <Loading />}
      {songs.isError && (
        <div className="card-body">
          <ErrorView error={songs.error} />
        </div>
      )}
      {songs.data && items.length === 0 && <Empty>{t(empty)}</Empty>}
      {items.length > 0 && (
        <div className="table-wrap">
          <table className="table">
            <thead>
              <tr>
                <th>{t("library.sort.title")}</th>
                <th>{t("library.sort.artist")}</th>
                <th className="hide-narrow">{t("library.sort.album")}</th>
              </tr>
            </thead>
            <tbody>
              {items.map((track) => (
                <tr key={track.id}>
                  <td className="title-cell">
                    <Link to={href(track.id)} className="ellipsis">
                      {track.title ?? track.path}
                    </Link>
                  </td>
                  <td className="ellipsis">{track.artist ?? "–"}</td>
                  <td className="hide-narrow ellipsis">{track.album ?? "–"}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
      {total > PAGE && (
        <div className="pager">
          <button
            type="button"
            className="btn small"
            disabled={page === 0}
            onClick={() => setSearch({ page: page - 1 || undefined })}
          >
            {t("library.previous")}
          </button>
          <span className="muted small num">
            {t("library.page", { page: page + 1, pages })} · {t("common.tracks", { count: total })}
          </span>
          <button
            type="button"
            className="btn small"
            disabled={page + 1 >= pages}
            onClick={() => setSearch({ page: page + 1 })}
          >
            {t("library.nextPage")}
          </button>
        </div>
      )}
    </Card>
  );
}

// Covers to check ------------------------------------------------------------

function CoversToCheck() {
  const albums = useQuery({
    queryKey: ["cover-candidates"],
    queryFn: () => unwrap(api.GET("/api/v1/covers/candidates")),
  });
  if (albums.isPending) {
    return <Loading />;
  }
  if (albums.isError) {
    return <ErrorView error={albums.error} />;
  }
  if (albums.data.length === 0) {
    return null;
  }
  return (
    <div className="stack">
      <h2 style={{ margin: 0 }}>{t("todo.coversToCheck")}</h2>
      <p className="muted" style={{ margin: 0 }}>
        {t("review.coversIntro")}
      </p>
      {albums.data.map((album) => (
        <CoverAlbum key={album.album_key} album={album} />
      ))}
    </div>
  );
}

function CoverAlbum({ album }: Readonly<{ album: Schemas["CandidateAlbum"] }>) {
  const canEdit = useCan("operator");
  const client = useQueryClient();
  const refresh = () => {
    void client.invalidateQueries({ queryKey: ["cover-candidates"] });
    void client.invalidateQueries({ queryKey: ["tracks"] });
    void client.invalidateQueries({ queryKey: TODO_COUNTS });
  };
  const refuse = useAction(
    () => done(api.POST("/api/v1/covers/candidates/refuse", { body: { album_key: album.album_key } })),
    { success: t("review.coversRefused"), onSuccess: refresh },
  );
  return (
    <Card
      title={
        <div style={{ minWidth: 0 }}>
          <h2 className="ellipsis">
            <Link to={`/library/${album.track_id}`}>{album.album ?? t("track.noTags")}</Link>
          </h2>
          <div className="muted small ellipsis">
            {album.artist} · {t("review.albumTracks", { count: album.tracks })}
          </div>
        </div>
      }
    >
      <CoverCandidates
        candidates={album.candidates}
        onAccepted={refresh}
        extra={
          canEdit && (
            <button type="button" className="btn small" disabled={refuse.isPending} onClick={() => refuse.mutate(undefined)}>
              {t("review.refuseCovers")}
            </button>
          )
        }
      />
    </Card>
  );
}

// Identifications -----------------------------------------------------------

/** Damaged files of the library, reviewed one after another. */
function DamagedTracks() {
  const canEdit = useCan("operator");
  const [reviewing, setReviewing] = useState<number | null>(null);
  const tracks = useQuery({
    queryKey: ["tracks", "damaged"],
    queryFn: () => unwrap(api.GET("/api/v1/tracks", { params: { query: { filter: "damaged", limit: 200 } } })),
  });
  if (tracks.isPending) {
    return <Loading />;
  }
  if (tracks.isError) {
    return <ErrorView error={tracks.error} />;
  }
  const ids = tracks.data.items.map((track) => track.id);
  if (ids.length === 0) {
    return (
      <Card>
        <Empty>{t("todo.noDamaged")}</Empty>
      </Card>
    );
  }
  return (
    <div className="stack">
      <div className="row between">
        <p className="muted" style={{ margin: 0 }}>
          {t("todo.damagedIntro", { count: tracks.data.total })}
        </p>
        {canEdit && (
          <button type="button" className="btn small primary" onClick={() => setReviewing(0)}>
            <Icon name="check" size={14} /> {t("buffer.review.action")}
          </button>
        )}
      </div>
      <Card flush>
        <ul className="review-list" style={{ maxHeight: "none", border: 0 }}>
          {tracks.data.items.map((track, at) => (
            <li key={track.id}>
              <TrackRow track={track} onOpen={canEdit ? () => setReviewing(at) : undefined} />
            </li>
          ))}
        </ul>
      </Card>
      {reviewing !== null && <LibraryReviewDialog queue={ids} start={reviewing} onClose={() => setReviewing(null)} />}
    </div>
  );
}

/** A track to check: its name (a link to its page), and the button that
 * opens the review. */
function TrackRow({
  track,
  onOpen,
}: Readonly<{ track: Schemas["TrackSummary"]; onOpen: (() => void) | undefined }>) {
  return (
    <>
      <span className="candidate-text">
        <Link to={`/library/${track.id}`} className="ellipsis">
          {trackName(track)}
        </Link>
        <span className="ellipsis muted small">
          <code>{track.path}</code> · {duration(track.duration_ms)}
        </span>
      </span>
      <span />
      {onOpen && (
        <button type="button" className="btn small" onClick={onOpen}>
          {t("buffer.review.action")}
        </button>
      )}
    </>
  );
}

function Identifications() {
  const canEdit = useCan("operator");
  const [reviewing, setReviewing] = useState<number | null>(null);
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
          {rich("review.nothing", { buffer: <Link to="/import-music">{t("review.nothingBuffer")}</Link> })}
        </Empty>
      </Card>
    );
  }
  return (
    <div className="stack">
      <div className="row between">
        <p className="muted" style={{ margin: 0 }}>
          {t("review.intro", { count: review.data.total })}
        </p>
        {canEdit && (
          <button type="button" className="btn small primary" onClick={() => setReviewing(0)}>
            <Icon name="check" size={14} /> {t("buffer.review.action")}
          </button>
        )}
      </div>
      <Card flush>
        <ul className="review-list" style={{ maxHeight: "none", border: 0 }}>
          {review.data.items.map((item, at) => (
            <li key={item.track.id}>
              <TrackRow track={item.track} onOpen={canEdit ? () => setReviewing(at) : undefined} />
            </li>
          ))}
        </ul>
      </Card>
      {reviewing !== null && (
        <LibraryReviewDialog
          queue={review.data.items.map((item) => item.track.id)}
          start={reviewing}
          onClose={() => setReviewing(null)}
        />
      )}
    </div>
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
        title={t("review.organizeTitle")}
        actions={
          isAdmin && (
            <button
              type="button"
              className="btn primary"
              disabled={dryRun.isPending || plan.data?.status === "pending" || plan.data?.status === "running"}
              onClick={() => dryRun.mutate(undefined)}
            >
              {plan.data ? t("review.newDryRun") : t("review.dryRun")}
            </button>
          )
        }
      >
        <div className="stack">
          <p style={{ margin: 0 }}>
            {rich("review.organizeHelp", {
              layout: <code>{settings.data?.layout ?? "…"}</code>,
              link: <Link to="/settings?tab=library">{t("review.changeIt")}</Link>,
            })}
          </p>
          <p className="muted small" style={{ margin: 0 }}>
            {t("review.dryRunHelp")}
          </p>
          {!isAdmin && <p className="muted small">{t("review.adminsOnly")}</p>}
        </div>
      </Card>

      {plan.data && (
        <Card
          title={t("review.dryRunAt", { date: dateTime(plan.data.created_at_unix_ms) })}
          actions={
            <>
              <TaskState task={plan.data} />
              {isAdmin && plan.data.status === "done" && moves.length > 0 && !applyId && (
                <ConfirmButton
                  className="btn primary"
                  confirm={t("review.moveFiles", { count: moves.length })}
                  question={<p>{t("review.moveQuestion", { count: moves.length })}</p>}
                  onConfirm={() => apply.mutate(plan.data?.id ?? 0)}
                >
                  {t("review.applyMoves", { count: moves.length })}
                </ConfirmButton>
              )}
            </>
          }
          flush
        >
          {applied.data && (
            <div className="card-body stack">
              <div className="row between">
                <strong>{t("review.applying")}</strong>
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
                {t("review.moves", { count: moves.length })}, {t("review.inPlace", { count: plan.data.plan.unchanged })}
                {plan.data.plan.unreadable.length > 0 &&
                  `, ${t("review.unreadable", { count: plan.data.plan.unreadable.length })}`}
                {t("library.end")}
              </div>
              {moves.length > 0 && (
                <div className="table-wrap">
                  <table className="table">
                    <thead>
                      <tr>
                        <th>{t("review.from")}</th>
                        <th>{t("review.to")}</th>
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
                  <span className="muted small">{t("queue.more", { count: moves.length - shown })}</span>
                  <button type="button" className="btn small" onClick={() => setShown((n) => n + 500)}>
                    {t("review.showMore")}
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

function BatchReport({ report }: Readonly<{ report: Schemas["BatchReport"] }>) {
  return (
    <div className="stack">
      <span>
        {t("review.batchReport", { batch: String(report.batch_id), count: report.done })}
        {report.skipped.length > 0 && `, ${t("review.skipped", { count: report.skipped.length })}`}
        {t("library.end")}
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

const KINDS: Record<string, PlainKey> = {
  import: "review.kind.import",
  organize: "review.kind.organize",
  undo: "review.kind.undo",
  review: "review.kind.review",
  edit: "review.kind.edit",
  delete: "review.kind.delete",
  purge: "review.kind.purge",
};

function kindLabel(kind: string): string {
  const key = KINDS[kind];
  return key ? t(key) : kind;
}

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
      let message = t("review.undone");
      if (report) {
        message = report.skipped.length
          ? t("review.undoneSkipped", { count: report.done, skipped: report.skipped.length })
          : t("review.undoneCount", { count: report.done });
      }
      toast(message);
      void client.invalidateQueries({ queryKey: ["library-batches"] });
      void client.invalidateQueries({ queryKey: ["review"] });
    } else if (status === "failed") {
      toast(undoing.data?.error ?? t("review.undoFailed"), true);
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
          {t("review.undoing")} <TaskState task={undoing.data} />
        </Banner>
      )}
      <Card flush>
        {batches.data.length === 0 ? (
          <Empty>{t("review.noChange")}</Empty>
        ) : (
          <table className="table">
            <thead>
              <tr>
                <th>{t("review.batch")}</th>
                <th>{t("review.when")}</th>
                <th>{t("review.files")}</th>
                <th>{t("review.state")}</th>
                <th>
                  <span className="visually-hidden">{t("library.actions")}</span>
                </th>
              </tr>
            </thead>
            <tbody>
              {batches.data.map((batch) => (
                <tr key={batch.id}>
                  <td>
                    #{batch.id} {kindLabel(batch.kind)}
                    {batch.undoes && <span className="muted"> {t("review.of", { batch: String(batch.undoes) })}</span>}
                  </td>
                  <td>{dateTime(batch.created_at_unix_ms)}</td>
                  <td className="num">{number(batch.files)}</td>
                  <td>
                    {!!batch.undone_by && (
                      <span className="badge">{t("review.undoneBy", { batch: String(batch.undone_by) })}</span>
                    )}
                    {!batch.undone_by &&
                      (batch.finished_at_unix_ms ? (
                        <span className="badge good">{t("review.done")}</span>
                      ) : (
                        <span className="badge warn">{t("review.unfinished")}</span>
                      ))}
                  </td>
                  <td className="actions">
                    {isAdmin && batch.kind !== "undo" && batch.kind !== "purge" && !batch.undone_by && batch.files > 0 && (
                      <ConfirmButton
                        className="btn small"
                        confirm={t("review.kind.undo")}
                        disabled={busy}
                        question={<p>{t("review.undoQuestion", { count: batch.files, batch: String(batch.id) })}</p>}
                        onConfirm={() => undo.mutate(batch.id)}
                      >
                        <Icon name="undo" size={14} /> {t("review.kind.undo")}
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
