// SPDX-License-Identifier: AGPL-3.0-or-later

// The review of library tracks, as files are reviewed on Import music:
// accept the release found, pick another one, search MusicBrainz again, or
// give the track its tags by hand ("not on MusicBrainz"); put it in a
// category; move it to the trash. Damage found in its file is shown.

import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useRef, useState } from "react";
import { type Schemas, api, unwrap } from "../api";
import { useTask } from "../components/Task";
import { type TagValue, TagValueInput, suggestedField } from "../components/TagValueInput";
import { Icon, Modal, useToast } from "../components/ui";
import { type PlainKey, t } from "../i18n";
import { duration } from "../lib/format";
import { Link } from "../lib/router";
import { useAction } from "../lib/session";
import { AlbumPicker, CandidateText, ChooseHelp, FixRow, KindFilter, albumRows, fromName, releaseLine, tagText } from "./BufferReview";

type Candidate = Schemas["ReviewCandidate"];
type AlbumKind = Schemas["AlbumKind"];
type Detail = Schemas["TrackDetail"];
type TagField = Schemas["TagField"];
type Tags = Schemas["StagedTags"];

/** The tags compared on a match, and where each is read. */
const MATCH_FIELDS: { key: keyof Tags; field: TagField; label: PlainKey; numeric?: boolean }[] = [
  { key: "title", field: "title", label: "buffer.tag.title" },
  { key: "artist", field: "artist", label: "buffer.tag.artist" },
  { key: "album", field: "album", label: "buffer.tag.album" },
  { key: "album_artist", field: "album_artist", label: "buffer.tag.albumArtist" },
  { key: "date", field: "year", label: "buffer.tag.year", numeric: true },
  { key: "track", field: "track", label: "buffer.tag.track", numeric: true },
  { key: "disc", field: "disc", label: "buffer.tag.disc", numeric: true },
];

/** Runs a release preview task and waits for its tags. */
async function previewOf(id: number, candidate: Candidate): Promise<Schemas["ReleasePreview"]> {
  const queued = await unwrap(
    api.POST("/api/v1/tracks/{id}/review/preview", {
      params: { path: { id } },
      body: { release_id: candidate.release_id, recording_id: candidate.recording_id },
    }),
  );
  for (;;) {
    const done = await unwrap(api.GET("/api/v1/library/tasks/{id}", { params: { path: { id: queued.id } } }));
    if (done.status === "done" && done.preview) {
      return done.preview;
    }
    if (done.status === "done" || done.status === "failed") {
      throw new Error(done.error ?? t("buffer.review.taskFailed"));
    }
    await new Promise((resolve) => setTimeout(resolve, 700));
  }
}

const FIELDS: { field: TagField; label: PlainKey; size: string; numeric?: boolean }[] = [
  { field: "title", label: "buffer.tag.title", size: "full" },
  { field: "artist", label: "buffer.tag.artist", size: "wide" },
  { field: "album", label: "buffer.tag.album", size: "wide" },
  { field: "album_artist", label: "buffer.tag.albumArtist", size: "wide" },
  { field: "year", label: "buffer.tag.year", size: "narrow", numeric: true },
  { field: "track", label: "buffer.tag.track", size: "narrow", numeric: true },
  { field: "disc", label: "buffer.tag.disc", size: "narrow", numeric: true },
];

type Draft = Partial<Record<TagField, string>>;

function draftOf(detail: Detail | undefined): Draft {
  const s = detail;
  const text = (value: string | number | null | undefined) => (value === null || value === undefined ? "" : String(value));
  return {
    title: text(s?.title),
    artist: text(s?.artist),
    album: text(s?.album),
    album_artist: text(s?.album_artist),
    year: text(s?.year),
    track: text(detail?.file?.track_number),
    disc: text(detail?.file?.disc_number),
  };
}

function badNumber(text: string | undefined): boolean {
  const value = text?.trim() ?? "";
  return value !== "" && (!/^\d+$/.test(value) || Number(value) < 1);
}

const DAMAGE: Record<string, PlainKey> = {
  broken_seek_table: "buffer.flag.seekTableHelp",
  damaged_audio: "buffer.flag.damagedHelp",
  mixed_streams: "buffer.flag.mixedHelp",
};

/** The release a track is matched to (or proposed), among its candidates. */
function currentOf(detail: Detail | undefined): Candidate | undefined {
  const id = detail?.identification;
  if (!id?.release_mbid) {
    return undefined;
  }
  return (
    detail?.identification_candidates.find(
      (c) => c.release_id === id.release_mbid && c.recording_id === id.recording_mbid,
    ) ?? detail?.identification_candidates.find((c) => c.release_id === id.release_mbid)
  );
}

type View = "match" | "search" | "manual";

/** The library's tracks reviewed one after another. */
export function LibraryReviewDialog({
  queue,
  start,
  onClose,
}: Readonly<{ queue: number[]; start: number; onClose: () => void }>) {
  const client = useQueryClient();
  const toast = useToast();
  const [index, setIndex] = useState(start);
  const id = queue[index] ?? 0;
  const detail = useQuery({
    queryKey: ["track", id],
    queryFn: () => unwrap(api.GET("/api/v1/tracks/{id}", { params: { path: { id } } })),
  });
  const library = useQuery({
    queryKey: ["library-settings"],
    queryFn: () => unwrap(api.GET("/api/v1/settings/library")),
  });
  const categories = useQuery({ queryKey: ["categories"], queryFn: () => unwrap(api.GET("/api/v1/categories")) });
  const track = detail.data;
  // A search result picked: reviewed, and fixed, before it is applied.
  const [picked, setPicked] = useState<Candidate | null>(null);
  const current = picked ?? currentOf(track);
  const [view, setView] = useState<View>("search");
  const [confirming, setConfirming] = useState(false);
  const [releasesOpen, setReleasesOpen] = useState(false);
  const [query, setQuery] = useState({ title: "", artist: "", album: "" });
  const [searched, setSearched] = useState(false);
  const [kinds, setKinds] = useState<AlbumKind[]>(["studio"]);
  const [draft, setDraft] = useState<Draft>({});
  // Fields fixed on top of the release's tags: `null` removes one.
  const [fixes, setFixes] = useState<Record<string, string | null>>({});
  const [tagsOpen, setTagsOpen] = useState(false);
  const [lastCategory, setLastCategory] = useState<number | null>(null);
  // What the running task does once it is done.
  const [taskId, setTaskId] = useState<number | null>(null);
  const after = useRef<(() => void) | null>(null);
  const task = useTask(taskId);
  const audio = useRef<HTMLAudioElement>(null);

  const shown = useRef<number | null>(null);
  useEffect(() => {
    if (!track || shown.current === track.id) {
      return;
    }
    shown.current = track.id;
    setView(currentOf(track) || track.identification?.status === "identified" ? "match" : "search");
    setConfirming(false);
    setReleasesOpen(false);
    setSearched(false);
    setQuery({ title: track.title ?? "", artist: track.artist ?? "", album: track.album ?? "" });
    setDraft(draftOf(track));
    setFixes({});
    setTagsOpen(false);
    setPicked(null);
  }, [track]);
  // Fixes belong to one release.
  // biome-ignore lint/correctness/useExhaustiveDependencies: reset when the release changes.
  useEffect(() => setFixes({}), [current?.release_id, current?.recording_id]);
  const preview = useQuery({
    queryKey: ["review-preview", id, current?.release_id, current?.recording_id],
    enabled: view === "match" && current !== undefined && taskId === null,
    staleTime: Number.POSITIVE_INFINITY,
    retry: false,
    queryFn: () => (current ? previewOf(id, current) : Promise.reject(new Error("no release"))),
  });

  const refresh = () => {
    for (const key of [["track", id], ["review"], ["todo-counts"], ["tracks"], ["library-batches"]]) {
      void client.invalidateQueries({ queryKey: key });
    }
  };
  useEffect(() => {
    const status = task.data?.status;
    if (status !== "done" && status !== "failed") {
      return;
    }
    if (status === "failed") {
      toast(task.data?.error ?? t("buffer.review.taskFailed"), true);
    }
    setTaskId(null);
    refresh();
    const then = after.current;
    after.current = null;
    if (status === "done") {
      then?.();
    }
    // biome-ignore lint/correctness/useExhaustiveDependencies: once per finished task.
  }, [task.data?.status]);

  /** Runs a library task, then `then` once it is done. */
  const run = (started: Promise<Schemas["LibraryTask"]>, then?: () => void) => {
    after.current = then ?? null;
    started.then((queued) => setTaskId(queued.id)).catch((error: unknown) => toast(String(error), true));
  };

  const next = () => {
    if (index + 1 < queue.length) {
      setIndex(index + 1);
    } else {
      onClose();
    }
  };
  const move = (by: number) => setIndex((index + by + queue.length) % queue.length);

  const categorize = () => {
    if (track && track.categories.length === 0 && lastCategory !== null) {
      void api.POST("/api/v1/tracks/categories", {
        body: { selection: { track_ids: [id] }, add: [{ category_id: lastCategory, weight: 1 }] },
      });
    }
  };
  const apply = (candidate: Candidate, then?: () => void) =>
    run(
      unwrap(
        api.POST("/api/v1/tracks/{id}/review/apply", {
          params: { path: { id } },
          body: { release_id: candidate.release_id, recording_id: candidate.recording_id, fixes },
        }),
      ),
      then,
    );
  const acceptMatch = () => {
    categorize();
    const same = current?.release_id === track?.identification?.release_mbid;
    if (!current || (track?.identification?.status === "identified" && same && Object.keys(fixes).length === 0)) {
      next();
      return;
    }
    apply(current, next);
  };
  const runSearch = (wanted: AlbumKind[] = kinds) => {
    setSearched(true);
    run(
      unwrap(
        api.POST("/api/v1/tracks/{id}/review/search", {
          params: { path: { id } },
          body: {
            title: query.title.trim(),
            artist: query.artist.trim(),
            ...(query.album.trim() ? { album: query.album.trim() } : {}),
            kinds: wanted,
          },
        }),
      ),
    );
  };
  const keep = useAction(() => unwrap(api.POST("/api/v1/tracks/{id}/review/keep", { params: { path: { id } } })), {
    invalidate: [["track", id], ["review"], ["todo-counts"]],
  });
  const acceptManual = async () => {
    if (!track) {
      return;
    }
    categorize();
    const base = draftOf(track);
    const set: Record<string, string | null> = {};
    for (const { field } of FIELDS) {
      if ((draft[field] ?? "").trim() !== (base[field] ?? "").trim()) {
        set[field] = draft[field]?.trim() || null;
      }
    }
    const done = () => keep.mutate(undefined, { onSuccess: next });
    if (Object.keys(set).length === 0) {
      done();
    } else if (library.data?.write_tags === false) {
      await unwrap(api.POST("/api/v1/tracks/tag-overrides", { body: { selection: { track_ids: [id] }, set } }));
      refresh();
      done();
    } else {
      run(unwrap(api.POST("/api/v1/tracks/tags", { body: { selection: { track_ids: [id] }, set } })), done);
    }
  };
  const setCategory = async (value: string) => {
    if (!track) {
      return;
    }
    const chosen = value ? Number(value) : null;
    setLastCategory(chosen);
    const remove = track.categories.map((c) => c.category_id).filter((c) => c !== chosen);
    if (remove.length > 0) {
      await api.POST("/api/v1/tracks/categories", { body: { selection: { track_ids: [id] }, remove } });
    }
    if (chosen !== null) {
      await api.POST("/api/v1/tracks/categories", {
        body: { selection: { track_ids: [id] }, add: [{ category_id: chosen, weight: 1 }] },
      });
    }
    refresh();
  };
  const discard = () => {
    setConfirming(false);
    run(unwrap(api.POST("/api/v1/tracks/delete", { body: { track_ids: [id] } })), () => {
      toast(t("buffer.review.trashed"));
      next();
    });
  };

  const busy = taskId !== null || keep.isPending || !track;
  const keys = (event: globalThis.KeyboardEvent) => {
    const target = event.target as HTMLElement;
    if (
      ["INPUT", "SELECT", "TEXTAREA", "AUDIO"].includes(target.tagName) ||
      target.isContentEditable ||
      event.metaKey ||
      event.ctrlKey ||
      event.altKey ||
      event.defaultPrevented
    ) {
      return;
    }
    const actions: Record<string, (() => void) | undefined> = {
      arrowleft: () => move(-1),
      arrowright: () => move(1),
      " ": () => {
        const player = audio.current;
        if (player) {
          void (player.paused ? player.play() : player.pause());
        }
      },
      d: () => setConfirming(true),
      s: () => setView("search"),
      m: () => {
        setDraft(draftOf(track));
        setView("manual");
      },
      a: view === "match" ? acceptMatch : view === "manual" ? () => void acceptManual() : undefined,
    };
    const action = busy ? undefined : actions[event.key.toLowerCase()];
    if (action) {
      event.preventDefault();
      action();
    }
  };
  // The shortcuts work wherever the focus is in the dialog (its close
  // button, the footer), not only in its body.
  const keysNow = useRef(keys);
  keysNow.current = keys;
  useEffect(() => {
    const listener = (event: globalThis.KeyboardEvent) => keysNow.current(event);
    window.addEventListener("keydown", listener);
    return () => window.removeEventListener("keydown", listener);
  }, []);

  const candidates = track?.identification_candidates ?? [];
  const albums = albumRows(candidates, kinds);
  const releases = albumRows(candidates, ["studio", "live", "compilation", "single", "other"]).rows;
  const length = track?.duration_ms ?? 0;
  const named = track ? fromName(track.path) : null;
  const offered =
    named && Object.entries(named).some(([field, value]) => draft[field as TagField]?.trim() !== value) ? named : null;
  const identified = track?.identification?.status === "identified";
  const kept = track?.identification?.status === "kept";

  let banner: { kind: string; title: string; text: string };
  if (view === "manual") {
    banner = { kind: "plain", title: t("buffer.review.manualTitle"), text: t("buffer.review.manualTextLibrary") };
  } else if (view === "match") {
    banner = identified
      ? { kind: "good", title: t("buffer.review.identifiedTitle"), text: t("buffer.review.identifiedText") }
      : {
          kind: "warn",
          title: t("buffer.review.foundTitle"),
          text: track?.identification?.reason ?? "",
        };
  } else if (kept) {
    banner = { kind: "plain", title: t("buffer.review.keptTitle"), text: t("buffer.review.keptText") };
  } else {
    banner = {
      kind: candidates.length > 0 ? "plain" : "warn",
      title: candidates.length > 0 ? t("buffer.review.searchTitle") : t("buffer.review.notFoundTitle"),
      text: track?.identification?.reason ?? t("buffer.review.notIdentified"),
    };
  }

  const footer = confirming ? (
    <>
      <span className="bad" style={{ marginRight: "auto", alignSelf: "center" }}>
        <strong>{t("buffer.review.trashQuestion")}</strong> {t("buffer.review.trashHelp")}
      </span>
      <button type="button" className="btn" onClick={() => setConfirming(false)}>
        {t("buffer.review.keepIt")}
      </button>
      <button type="button" className="btn danger" onClick={discard}>
        {t("buffer.review.trash")}
      </button>
    </>
  ) : (
    <>
      <button type="button" className="btn ghost danger-text" disabled={busy} onClick={() => setConfirming(true)}>
        {t("buffer.review.trash")}
        <kbd>D</kbd>
      </button>
      <span style={{ flex: 1 }} />
      {view === "match" && (
        <>
          <button type="button" className="btn" disabled={busy} onClick={() => setView("search")}>
            {t("buffer.review.wrongSong")}
            <kbd>S</kbd>
          </button>
          <button
            type="button"
            className="btn"
            disabled={busy}
            onClick={() => {
              setDraft(draftOf(track));
              setView("manual");
            }}
          >
            {t("buffer.review.notOnMb")}
            <kbd>M</kbd>
          </button>
          <button type="button" className="btn primary" disabled={busy} onClick={acceptMatch}>
            {Object.keys(fixes).length > 0
              ? t("buffer.review.acceptFixes", { count: Object.keys(fixes).length })
              : identified && current?.release_id === track?.identification?.release_mbid
                ? t("buffer.review.keepMatch")
                : t("buffer.review.acceptMatch")}
            <kbd>A</kbd>
          </button>
        </>
      )}
      {view === "search" && (
        <>
          {current && (
            <button type="button" className="btn" disabled={busy} onClick={() => setView("match")}>
              ‹ {t("buffer.review.backToMatch")}
            </button>
          )}
          <button
            type="button"
            className={searched && candidates.length === 0 ? "btn primary" : "btn"}
            disabled={busy}
            onClick={() => {
              setDraft(draftOf(track));
              setView("manual");
            }}
          >
            {t("buffer.review.manual")}
            <kbd>M</kbd>
          </button>
        </>
      )}
      {view === "manual" && (
        <>
          <button type="button" className="btn" disabled={busy} onClick={() => setView("search")}>
            ‹ {t("buffer.review.backToSearch")}
            <kbd>S</kbd>
          </button>
          <button
            type="button"
            className="btn primary"
            disabled={busy || FIELDS.some(({ field, numeric }) => numeric && badNumber(draft[field]))}
            onClick={() => void acceptManual()}
          >
            {t("buffer.review.acceptTags")}
            <kbd>A</kbd>
          </button>
        </>
      )}
    </>
  );

  return (
    <Modal title={t("buffer.review.title", { index: index + 1, total: queue.length })} open onClose={onClose} wide footer={footer}>
      {track ? (
        // biome-ignore lint: the shortcuts are the dialog's, not one control's.
        <div className="stack review" tabIndex={-1} data-autofocus>
          <div className="row" style={{ gap: "0.5rem" }}>
            <button type="button" className="btn small" aria-label={t("buffer.review.previous")} onClick={() => move(-1)}>
              ‹<kbd>←</kbd>
            </button>
            <Link to={`/library/${id}`} className="mono small ellipsis" style={{ flex: 1 }}>
              {track.path}
            </Link>
            <span className="muted small">{duration(length)}</span>
            <button type="button" className="btn small" aria-label={t("buffer.review.next")} onClick={() => move(1)}>
              <kbd style={{ margin: "0 0.35rem 0 0" }}>→</kbd>›
            </button>
          </div>
          <audio ref={audio} key={id} controls preload="metadata" src={`/api/v1/tracks/${id}/audio`} style={{ width: "100%" }}>
            <track kind="captions" />
          </audio>

          {track.damage.length > 0 && (
            <div className="review-banner bad">
              <strong>{t("buffer.review.damagedTitle")}</strong>
              <ul style={{ margin: 0, paddingLeft: "1.1rem", flexBasis: "100%" }}>
                {track.damage.map((flag) => (
                  <li key={flag}>{t(DAMAGE[flag] ?? "buffer.flag.unreadableHelp")}</li>
                ))}
              </ul>
              <span>{t("buffer.review.damagedTextLibrary")}</span>
            </div>
          )}

          <div className={`review-banner ${banner.kind}`}>
            <strong>{banner.title}</strong>
            <span>{banner.text}</span>
          </div>

          {taskId !== null ? (
            <span className="row small muted">
              <span className="spinner" /> {t("buffer.review.working")}
            </span>
          ) : (
            <>
              {view === "match" && (
                <>
                  {!current ? (
                    <div className="review-compare" style={{ gridTemplateColumns: "7rem minmax(0, 1fr)" }}>
                      {FIELDS.slice(0, 5).map(({ field, label }) => (
                        <span key={field} style={{ display: "contents" }}>
                          <span className="muted">{t(label)}</span>
                          <span>{draftOf(track)[field] || "—"}</span>
                        </span>
                      ))}
                    </div>
                  ) : preview.isPending ? (
                    <span className="row small muted">
                      <span className="spinner" /> {t("buffer.review.previewing")}
                    </span>
                  ) : preview.isError ? (
                    <span className="row small bad">
                      {preview.error.message}
                      <button type="button" className="link-button" onClick={() => void preview.refetch()}>
                        {t("buffer.review.retry")}
                      </button>
                    </span>
                  ) : (
                    (() => {
                      const rows = MATCH_FIELDS.map((row) => ({
                        ...row,
                        own: tagText(preview.data.own[row.key], Boolean(row.numeric)),
                        mb: tagText(preview.data.applied[row.key], Boolean(row.numeric)),
                      }));
                      const agree = rows.every((row) => row.own === row.mb);
                      const edited = Object.keys(fixes);
                      if (agree && edited.length === 0 && !tagsOpen) {
                        return (
                          <button type="button" className="btn small review-agree" onClick={() => setTagsOpen(true)}>
                            ✓ {t("buffer.review.allAgree")} ▸
                          </button>
                        );
                      }
                      return (
                        <>
                          <div className="review-compare">
                            <span className="head">{t("buffer.tag")}</span>
                            <span className="head">{t("buffer.inFile")}</span>
                            <span className="head">{t("buffer.review.importedWith")}</span>
                            <span className="head" />
                            {rows.map(({ field, label, own, mb }) => {
                              const fixed = field in fixes;
                              const value = fixed ? (fixes[field] ?? "") : mb;
                              return (
                                <FixRow
                                  key={`${id}-${field}-${value}`}
                                  label={t(label)}
                                  own={own}
                                  value={value}
                                  fixed={fixed}
                                  differs={own !== value}
                                  disabled={busy}
                                  onFix={(text) =>
                                    setFixes((before) => {
                                      const after = { ...before };
                                      if (text.trim() === mb) {
                                        delete after[field];
                                      } else {
                                        after[field] = text.trim() || null;
                                      }
                                      return after;
                                    })
                                  }
                                  onRevert={() =>
                                    setFixes((before) => {
                                      const after = { ...before };
                                      delete after[field];
                                      return after;
                                    })
                                  }
                                />
                              );
                            })}
                          </div>
                          <div className="row small" style={{ gap: "1rem" }}>
                            <span className="muted" style={{ flex: 1 }}>
                              {t("buffer.review.fixesHelpLibrary")}
                            </span>
                            {edited.length > 0 && (
                              <button type="button" className="link-button warn" disabled={busy} onClick={() => setFixes({})}>
                                ↺ {t("buffer.review.revertAll")}
                              </button>
                            )}
                          </div>
                        </>
                      );
                    })()
                  )}
                  {current && (
                    <>
                      <div className="row small" style={{ gap: "1rem" }}>
                        <span className="muted" style={{ flex: 1 }}>
                          {releaseLine(current)}
                        </span>
                        {releases.length > 1 && (
                          <button type="button" className="link-button" onClick={() => setReleasesOpen(!releasesOpen)}>
                            {releasesOpen
                              ? t("buffer.review.hideReleases")
                              : t("buffer.review.otherReleases", { count: releases.length })}
                          </button>
                        )}
                        <a href={`https://musicbrainz.org/release/${current.release_id}`} target="_blank" rel="noreferrer">
                          {t("buffer.review.onMusicBrainz")}
                        </a>
                      </div>
                      {releasesOpen && (
                        <AlbumPicker
                          candidates={candidates}
                          current={current}
                          kinds={kinds}
                          onKinds={setKinds}
                          busy={busy}
                          onPick={setPicked}
                        />
                      )}
                    </>
                  )}
                  {!identified && <span className="muted small">{t("buffer.review.applyHelp")}</span>}
                </>
              )}

              {view === "search" && (
                <>
                  <form
                    className="review-search"
                    onSubmit={(event) => {
                      event.preventDefault();
                      if (query.title.trim() && query.artist.trim()) {
                        runSearch();
                      }
                    }}
                  >
                    {(["title", "artist", "album"] as const).map((name) => (
                      <label key={name} className="field">
                        <span>
                          {t(name === "album" ? "buffer.review.albumOptional" : name === "title" ? "buffer.tag.title" : "buffer.tag.artist")}
                        </span>
                        <input className="input" value={query[name]} onChange={(event) => setQuery({ ...query, [name]: event.target.value })} />
                      </label>
                    ))}
                    <button type="submit" className="btn" disabled={busy || !query.title.trim() || !query.artist.trim()}>
                      <Icon name="search" size={14} /> {t("buffer.review.search")}
                    </button>
                  </form>
                  <KindFilter
                    value={kinds}
                    disabled={busy}
                    onChange={(next) => {
                      setKinds(next);
                      if (searched && query.title.trim() && query.artist.trim()) {
                        runSearch(next);
                      }
                    }}
                  />
                  {albums.widened && <span className="muted small">{t("buffer.review.kinds.widened")}</span>}
                  {albums.rows.length > 0 && (
                    <>
                      <ul className="review-list">
                        {albums.rows.map((candidate) => {
                          const off = Boolean(candidate.length_ms && length && Math.abs(candidate.length_ms - length) > 10_000);
                          return (
                            <li
                              key={`${candidate.recording_id}-${candidate.release_id}-${candidate.disc}-${candidate.track}`}
                              style={off ? { opacity: 0.5 } : undefined}
                            >
                              <CandidateText candidate={candidate} />
                              <span className={`small ${off ? "bad" : "good"}`}>
                                {candidate.length_ms
                                  ? t("buffer.review.lengths", { length: duration(candidate.length_ms), file: duration(length) })
                                  : ""}{" "}
                                · {t("buffer.percent", { score: Math.round(candidate.score * 100) })}
                              </span>
                              <button type="button" className="btn small" disabled={busy} onClick={() => {
                                  setPicked(candidate);
                                  setView("match");
                                }}
                              >
                                {t("buffer.use")}
                              </button>
                            </li>
                          );
                        })}
                      </ul>
                      <span className="muted small">{t("buffer.review.dimmed")}</span>
                      <ChooseHelp />
                    </>
                  )}
                  {searched && albums.rows.length === 0 && <div className="review-empty">{t("buffer.review.nothing")}</div>}
                </>
              )}

              {view === "manual" && (
                <>
                  {offered && (
                    <div className="row small" style={{ gap: "0.5rem" }}>
                      <span className="muted">
                        {t("buffer.fill.fromName", {
                          value: [offered.track, [offered.artist, offered.title].filter(Boolean).join(" – ")]
                            .filter(Boolean)
                            .join(". "),
                        })}
                      </span>
                      <button type="button" className="btn small" onClick={() => setDraft({ ...draft, ...offered })}>
                        {t("buffer.fill.use")}
                      </button>
                    </div>
                  )}
                  <div className="fill-form">
                    {FIELDS.map(({ field, label, size, numeric }) => {
                      const name = t(label);
                      const suggested = suggestedField(field);
                      const bad = numeric && badNumber(draft[field]);
                      const common = { "aria-label": name, "aria-invalid": bad || undefined, className: bad ? "input invalid" : "input" };
                      return (
                        <div key={field} className={`field ${size}`}>
                          <span>{name}</span>
                          {suggested ? (
                            <TagValueInput
                              field={suggested}
                              value={draft[field] ?? ""}
                              onValue={(value) => setDraft((all) => ({ ...all, [field]: value }))}
                              onPick={
                                suggested === "album"
                                  ? (album: TagValue) =>
                                      setDraft((all) => ({
                                        ...all,
                                        ...(!all.album_artist?.trim() && album.album_artist && album.album_artist !== all.artist?.trim()
                                          ? { album_artist: album.album_artist }
                                          : {}),
                                        ...(!all.year?.trim() && album.year ? { year: String(album.year) } : {}),
                                      }))
                                  : undefined
                              }
                              artist={draft.album_artist?.trim() || draft.artist}
                              {...common}
                            />
                          ) : (
                            <input
                              type="text"
                              inputMode={numeric ? "numeric" : undefined}
                              autoComplete="off"
                              value={draft[field] ?? ""}
                              onChange={(event) => setDraft((all) => ({ ...all, [field]: event.target.value }))}
                              {...common}
                            />
                          )}
                        </div>
                      );
                    })}
                  </div>
                  <div className="review-banner plain small">{t("buffer.review.notOnMbNote")}</div>
                </>
              )}
            </>
          )}

          <label className="row review-category">
            <strong>{t("buffer.review.category")}</strong>
            <select
              className="input"
              style={{ width: "auto", minWidth: "14rem" }}
              value={String(track.categories[0]?.category_id ?? lastCategory ?? "")}
              disabled={busy || !categories.data}
              onChange={(event) => void setCategory(event.target.value)}
            >
              <option value="">{t("buffer.review.noCategory")}</option>
              {categories.data?.map((c) => (
                <option key={c.id} value={c.id}>
                  {c.name}
                </option>
              ))}
            </select>
            <span className="muted small">{t("buffer.review.categoryHelp")}</span>
          </label>
          <span className="muted small">{t("buffer.review.keys")}</span>
        </div>
      ) : (
        <p className="muted">{t("ui.loading")}</p>
      )}
    </Modal>
  );
}
