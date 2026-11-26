// SPDX-License-Identifier: AGPL-3.0-or-later

// Reviewing the files of the buffer zone one after another: accept what
// MusicBrainz found (with fixes of your own on top), pick another release,
// search MusicBrainz again, or give the file its tags by hand; put it in a
// category; discard it.

import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useRef, useState } from "react";
import { type Schemas, api, unwrap } from "../api";
import { type TagValue, TagValueInput, suggestedField } from "../components/TagValueInput";
import { Icon, Modal } from "../components/ui";
import { type PlainKey, t } from "../i18n";
import { bytes, duration } from "../lib/format";
import { Link } from "../lib/router";
import { useAction } from "../lib/session";

type StagedFile = Schemas["StagedFile"];
type Candidate = Schemas["ReviewCandidate"];
type AlbumKind = Schemas["AlbumKind"];
type TagField = Schemas["TagField"];
type StagedTags = Schemas["StagedTags"];

/** Waiting for a decision: nobody accepted a release or the file's own tags. */
export function toReview(file: StagedFile): boolean {
  return file.status === "ready" && file.choice?.kind !== "release" && file.choice?.kind !== "own";
}

/** Accepted: a release, or its own tags, was settled on. */
export function isAccepted(file: StagedFile): boolean {
  return file.status === "ready" && (file.choice?.kind === "release" || file.choice?.kind === "own");
}

/** Found for sure, and its own tags already say the same. */
export function isSure(file: StagedFile): boolean {
  if (!toReview(file) || file.identification?.status !== "identified" || !file.release_id) {
    return false;
  }
  return FIELDS.every(({ key }) => same(file.own_tags?.[key], file.tags?.[key]));
}

/** The release and recording a file is matched to now, as a candidate. */
export function currentCandidate(file: StagedFile): Candidate | undefined {
  const recording = file.choice?.kind === "release" ? file.choice.recording_id : file.identification?.recording_id;
  return (
    file.candidates.find((c) => c.release_id === file.release_id && c.recording_id === recording) ??
    file.candidates.find((c) => c.release_id === file.release_id)
  );
}

/** The text a tag starts from in a field: a date's year for the year. */
export function tagText(value: string | number | boolean | null | undefined, numeric: boolean): string {
  if (value === null || value === undefined || value === false) {
    return "";
  }
  const text = String(value);
  return numeric ? (/^\d+/.exec(text)?.[0] ?? "") : text;
}

function same(a: string | number | boolean | null | undefined, b: string | number | boolean | null | undefined) {
  return tagText(a, false).trim() === tagText(b, false).trim();
}

/** The tags compared, and the field that edits each. */
const FIELDS: { key: keyof StagedTags; field: TagField; label: PlainKey; numeric?: boolean }[] = [
  { key: "title", field: "title", label: "buffer.tag.title" },
  { key: "artist", field: "artist", label: "buffer.tag.artist" },
  { key: "album", field: "album", label: "buffer.tag.album" },
  { key: "album_artist", field: "album_artist", label: "buffer.tag.albumArtist" },
  { key: "date", field: "year", label: "buffer.tag.year", numeric: true },
  { key: "track", field: "track", label: "buffer.tag.track", numeric: true },
  { key: "disc", field: "disc", label: "buffer.tag.disc", numeric: true },
];

/** The manual form's layout. */
const SIZES: Record<string, string> = {
  title: "full",
  artist: "wide",
  album: "wide",
  album_artist: "wide",
  year: "narrow",
  track: "narrow",
  disc: "narrow",
};

type Draft = Partial<Record<TagField, string>>;

function draftOf(tags: StagedTags | null | undefined): Draft {
  const draft: Draft = {};
  for (const { key, field, numeric } of FIELDS) {
    draft[field] = tagText(tags?.[key], Boolean(numeric));
  }
  return draft;
}

/** A number field that is not a positive whole number. */
function badNumber(text: string | undefined): boolean {
  const value = text?.trim() ?? "";
  return value !== "" && (!/^\d+$/.test(value) || Number(value) < 1);
}

/**
 * What a file's name says: "Artist - Title", with a track number before
 * ("01 - Artist - Title", "01. Title"). Nothing for a name that is only a
 * title.
 */
export function fromName(path: string): Draft | null {
  const base = (path.split("/").pop() ?? path)
    .replace(/\.[^.]+$/, "")
    .replaceAll("_", " ")
    .trim();
  const numbered = /^(\d{1,3})(?:\s*[-.)]\s*|\s+)(\S.*)$/.exec(base);
  const track = numbered?.[1] ? String(Number(numbered[1])) : undefined;
  const rest = numbered?.[2] ?? base;
  const [artist, ...title] = rest.split(/\s+[-–—]\s+/);
  if (artist && title.length > 0) {
    return { artist: artist.trim(), title: title.join(" - ").trim(), ...(track ? { track } : {}) };
  }
  return track ? { title: rest.trim(), track } : null;
}

export function releaseLine(candidate: Candidate): string {
  return [
    candidate.release_group_title ?? candidate.release_title ?? candidate.release_id,
    candidate.album_kind ? t(`buffer.review.kinds.${candidate.album_kind}`) : null,
    (candidate.album_date ?? candidate.date)?.slice(0, 4),
    candidate.track ? t("buffer.review.trackNumber", { track: candidate.track }) : null,
  ]
    .filter(Boolean)
    .join(" · ");
}

/** A length in seconds, as `m:ss`. */
function clock(ms: number | null | undefined): string {
  return ms ? duration(ms) : "–";
}

/** The damage a file can have, and what it means for it. */
const DAMAGE: Partial<Record<Schemas["StagedFlag"], PlainKey>> = {
  broken_seek_table: "buffer.flag.seekTableHelp",
  damaged_audio: "buffer.flag.damagedHelp",
  mixed_streams: "buffer.flag.mixedHelp",
  undecodable: "buffer.flag.unreadableHelp",
};

type View = "match" | "search" | "manual";

/** Where a file's review starts: its match, else the search. */
function firstView(file: StagedFile | undefined): View {
  return file?.release_id ? "match" : "search";
}

/**
 * The review of files one after another. `queue` is the files' order;
 * `start` the one it opens on.
 */
export function ReviewDialog({
  queue,
  start,
  files,
  onClose,
}: Readonly<{ queue: number[]; start: number; files: StagedFile[]; onClose: () => void }>) {
  const client = useQueryClient();
  const [index, setIndex] = useState(start);
  const id = queue[index] ?? 0;
  const file = files.find((f) => f.id === id);
  const [view, setView] = useState<View>(() => firstView(file));
  const [confirming, setConfirming] = useState(false);
  const [releasesOpen, setReleasesOpen] = useState(false);
  const [tagsOpen, setTagsOpen] = useState(false);
  const [query, setQuery] = useState({ title: "", artist: "", album: "" });
  const [searched, setSearched] = useState(false);
  const [kinds, setKinds] = useState<AlbumKind[]>(["studio"]);
  const [draft, setDraft] = useState<Draft>({});
  // Accepted or discarded in this review, before the list says so.
  const [done, setDone] = useState<Set<number>>(new Set());
  // The category last picked, offered for the next files.
  const [lastCategory, setLastCategory] = useState<number | null>(null);
  const audio = useRef<HTMLAudioElement>(null);
  const saving = useRef<Promise<unknown>>(Promise.resolve());

  // Another file: its review starts over.
  const shown = useRef<number | null>(null);
  useEffect(() => {
    if (!file || shown.current === file.id) {
      return;
    }
    shown.current = file.id;
    setView(firstView(file));
    setConfirming(false);
    setReleasesOpen(false);
    setTagsOpen(false);
    setSearched(false);
    const own = file.own_tags ?? file.tags;
    setQuery({ title: own?.title ?? "", artist: own?.artist ?? "", album: own?.album ?? "" });
    setDraft(draftOf(file.tags ?? file.own_tags));
  }, [file]);

  const invalidate = [["buffer"]];
  const choose = useAction(
    (body: Schemas["ChoiceRequest"]) =>
      unwrap(api.POST("/api/v1/staging/files/{id}/choice", { params: { path: { id } }, body })),
    { invalidate },
  );
  const tags = useAction(
    (body: { set?: Record<string, string | null>; reset?: TagField[] }) =>
      unwrap(api.POST("/api/v1/staging/files/tags", { body: { file_ids: [id], ...body } })),
    { invalidate },
  );
  const prepare = useAction(
    (body: Omit<Schemas["Preparation"], "file_ids">) =>
      unwrap(api.POST("/api/v1/staging/files/preparation", { body: { file_ids: [id], ...body } })),
    { invalidate },
  );
  const discard = useAction(
    () => unwrap(api.POST("/api/v1/staging/discard", { body: { file_ids: [id] } })),
    { invalidate, success: t("buffer.review.discarded") },
  );
  const categories = useQuery({ queryKey: ["categories"], queryFn: () => unwrap(api.GET("/api/v1/categories")) });

  const waiting = file?.status === "identifying";
  const busy = waiting || choose.isPending || tags.isPending || discard.isPending;

  /** The next file not done yet, else the end of the review. */
  const next = (doneNow?: number) => {
    const finished = new Set(done);
    if (doneNow !== undefined) {
      finished.add(doneNow);
      setDone(finished);
    }
    for (let step = 1; step <= queue.length; step += 1) {
      const at = (index + step) % queue.length;
      const candidate = files.find((f) => f.id === queue[at]);
      if (candidate && !finished.has(candidate.id) && toReview(candidate)) {
        setIndex(at);
        return;
      }
    }
    onClose();
  };
  const move = (by: number) => setIndex((index + by + queue.length) % queue.length);

  /** The category last picked goes to a file that has none. */
  const categorize = () => {
    if (file && file.categories.length === 0 && lastCategory !== null) {
      prepare.mutate({ add_categories: [{ category_id: lastCategory, weight: 1 }] });
    }
  };

  const current = file ? currentCandidate(file) : undefined;
  const releases = albumRows(file?.candidates ?? [], KINDS).rows;
  const edited = file ? FIELDS.filter(({ field }) => field in file.tag_edits) : [];
  const agree = file ? FIELDS.every(({ key }) => same(file.own_tags?.[key], file.tags?.[key])) : false;

  const acceptMatch = async () => {
    if (!file || !current) {
      return;
    }
    await saving.current;
    categorize();
    if (file.choice?.kind === "release" && file.choice.release_id === current.release_id) {
      next(file.id);
      return;
    }
    choose.mutate(
      { kind: "release", release_id: current.release_id, recording_id: current.recording_id },
      { onSuccess: () => next(file.id) },
    );
  };
  const pickRelease = (candidate: Candidate) => {
    setReleasesOpen(false);
    setView("match");
    choose.mutate({ kind: "release", release_id: candidate.release_id, recording_id: candidate.recording_id });
  };
  const runSearch = (wanted: AlbumKind[] = kinds) => {
    setSearched(true);
    choose.mutate({
      kind: "search",
      title: query.title.trim(),
      artist: query.artist.trim(),
      ...(query.album.trim() ? { album: query.album.trim() } : {}),
      kinds: wanted,
    });
  };
  const toManual = () => {
    setDraft(draftOf(file?.tags ?? file?.own_tags));
    setView("manual");
  };
  const acceptManual = () => {
    if (!file) {
      return;
    }
    const base = draftOf(file.tags ?? file.own_tags);
    const set: Record<string, string | null> = {};
    for (const { field } of FIELDS) {
      if ((draft[field] ?? "").trim() !== (base[field] ?? "").trim()) {
        set[field] = draft[field]?.trim() || null;
      }
    }
    categorize();
    const own = () => choose.mutate({ kind: "own" }, { onSuccess: () => next(file.id) });
    if (Object.keys(set).length > 0) {
      tags.mutate({ set }, { onSuccess: own });
    } else {
      own();
    }
  };
  const fixTag = (field: TagField, value: string, before: string) => {
    if (value.trim() === before.trim()) {
      return;
    }
    saving.current = tags.mutateAsync({ set: { [field]: value.trim() || null } }).catch(() => undefined);
  };
  const setCategory = (value: string) => {
    if (!file) {
      return;
    }
    const chosen = value ? Number(value) : null;
    setLastCategory(chosen);
    prepare.mutate({
      remove_categories: file.categories.map((c) => c.category_id).filter((c) => c !== chosen),
      ...(chosen !== null ? { add_categories: [{ category_id: chosen, weight: 1 }] } : {}),
    });
  };
  const onDiscard = () =>
    discard.mutate(undefined, {
      onSuccess: () => {
        setConfirming(false);
        void client.invalidateQueries({ queryKey: ["buffer"] });
        next(id);
      },
    });

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
    const key = event.key.toLowerCase();
    const run: Record<string, (() => void) | undefined> = {
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
      m: toManual,
      a: view === "match" ? () => void acceptMatch() : view === "manual" ? acceptManual : undefined,
    };
    const action = busy ? undefined : run[key];
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

  const length = file?.duration_ms ?? 0;
  const named = file ? fromName(file.name) : null;
  const offered =
    named && Object.entries(named).some(([field, value]) => draft[field as TagField]?.trim() !== value) ? named : null;
  const categoryId = file?.categories[0]?.category_id ?? lastCategory ?? "";
  // Heron marks the later copy; both say so.
  const twinId = file?.duplicate_of_file_id ?? files.find((f) => file && f.duplicate_of_file_id === file.id)?.id;
  const twin = twinId === undefined ? -1 : queue.indexOf(twinId);
  const inLibrary = file?.flags.includes("duplicate_in_library") || file?.flags.includes("same_recording_in_library");
  const damage = (file?.flags ?? []).filter((flag): flag is keyof typeof DAMAGE => flag in DAMAGE);
  const keyOff = file?.identification?.status === "off" && !file.release_id && view === "search" && !searched;
  const albums = albumRows(view === "search" && file ? file.candidates : [], kinds);
  const results = albums.rows;

  let banner: { kind: "good" | "warn" | "plain"; title: string; text: string };
  if (view === "manual") {
    banner = { kind: "plain", title: t("buffer.review.manualTitle"), text: t("buffer.review.manualText") };
  } else if (view === "match") {
    banner =
      file?.choice?.kind === "release" && file.identification?.status !== "identified"
        ? { kind: "good", title: t("buffer.review.pickedTitle"), text: t("buffer.review.pickedText") }
        : {
            kind: agree ? "good" : "warn",
            title: t("buffer.review.foundTitle"),
            text: agree
              ? t("buffer.review.foundAgree", { score: Math.round((file?.identification?.score ?? 0) * 100) })
              : t("buffer.review.foundDiffer", { score: Math.round((file?.identification?.score ?? 0) * 100) }),
          };
  } else if (keyOff) {
    banner = { kind: "warn", title: t("buffer.review.keyOffTitle"), text: t("buffer.review.keyOffText") };
  } else if (file?.release_id || results.length > 0) {
    banner = { kind: "plain", title: t("buffer.review.searchTitle"), text: t("buffer.review.searchText") };
  } else {
    banner = {
      kind: "warn",
      title: t("buffer.review.notFoundTitle"),
      text: file?.identification?.reason ?? t("buffer.review.notFoundText"),
    };
  }

  const plain = "btn";
  const footer = confirming ? (
    <>
      <span className="bad" style={{ marginRight: "auto", alignSelf: "center" }}>
        <strong>{t("buffer.review.discardQuestion")}</strong> {t("buffer.review.discardHelp")}
      </span>
      <button type="button" className={plain} onClick={() => setConfirming(false)}>
        {t("buffer.review.keepIt")}
      </button>
      <button type="button" className="btn danger" disabled={discard.isPending} onClick={onDiscard}>
        {t("buffer.review.discardFile")}
      </button>
    </>
  ) : (
    <>
      <button type="button" className="btn ghost danger-text" disabled={busy} onClick={() => setConfirming(true)}>
        {t("buffer.review.discard")}
        <kbd>D</kbd>
      </button>
      <span style={{ flex: 1 }} />
      {view === "match" && (
        <>
          <button type="button" className={plain} disabled={busy} onClick={() => setView("search")}>
            {t("buffer.review.wrongSong")}
            <kbd>S</kbd>
          </button>
          <button type="button" className={plain} disabled={busy} onClick={toManual}>
            {t("buffer.review.notOnMb")}
            <kbd>M</kbd>
          </button>
          <button type="button" className="btn primary" disabled={busy || !current} onClick={() => void acceptMatch()}>
            {edited.length > 0
              ? t("buffer.review.acceptFixes", { count: edited.length })
              : t("buffer.review.acceptMatch")}
            <kbd>A</kbd>
          </button>
        </>
      )}
      {view === "search" && (
        <>
          {file?.release_id && (
            <button type="button" className={plain} disabled={busy} onClick={() => setView("match")}>
              ‹ {t("buffer.review.backToMatch")}
            </button>
          )}
          <button
            type="button"
            className={searched && results.length === 0 && !waiting ? "btn primary" : plain}
            disabled={busy}
            onClick={toManual}
          >
            {t("buffer.review.manual")}
            <kbd>M</kbd>
          </button>
        </>
      )}
      {view === "manual" && (
        <>
          <button type="button" className={plain} disabled={busy} onClick={() => setView("search")}>
            ‹ {t("buffer.review.backToSearch")}
            <kbd>S</kbd>
          </button>
          <button
            type="button"
            className="btn primary"
            disabled={busy || FIELDS.some(({ field, numeric }) => numeric && badNumber(draft[field]))}
            onClick={acceptManual}
          >
            {t("buffer.review.acceptTags")}
            <kbd>A</kbd>
          </button>
        </>
      )}
    </>
  );

  return (
    <Modal
      title={t("buffer.review.title", { index: index + 1, total: queue.length })}
      open
      onClose={onClose}
      wide
      footer={footer}
    >
      {file ? (
        // biome-ignore lint: the shortcuts are the dialog's, not one control's.
        <div className="stack review" tabIndex={-1} data-autofocus>
          <div className="row" style={{ gap: "0.5rem" }}>
            <button type="button" className="btn small" aria-label={t("buffer.review.previous")} onClick={() => move(-1)}>
              ‹<kbd>←</kbd>
            </button>
            <span className="mono small ellipsis" style={{ flex: 1 }} title={file.name}>
              {file.name}
            </span>
            <span className="muted small">
              {clock(file.duration_ms)} · {bytes(file.size)}
            </span>
            <button type="button" className="btn small" aria-label={t("buffer.review.next")} onClick={() => move(1)}>
              <kbd style={{ margin: "0 0.35rem 0 0" }}>→</kbd>›
            </button>
          </div>
          <audio
            ref={audio}
            key={file.id}
            controls
            preload="metadata"
            src={`/api/v1/staging/files/${file.id}/audio`}
            style={{ width: "100%" }}
          >
            <track kind="captions" />
          </audio>

          {inLibrary && (
            <div className="review-banner bad">
              <strong>{t("buffer.review.inLibraryTitle")}</strong>
              <span>
                {file.flags.includes("same_recording_in_library")
                  ? t("buffer.review.sameRecording")
                  : t("buffer.review.sameFile")}
              </span>
              {file.duplicate_of_track_id && (
                <Link to={`/library/${file.duplicate_of_track_id}`}>{t("buffer.review.openTrack")}</Link>
              )}
              <button type="button" className="btn small danger" style={{ marginLeft: "auto" }} onClick={() => setConfirming(true)}>
                {t("buffer.review.discardCopy")}
              </button>
            </div>
          )}
          {twinId !== undefined && (
            <div className="review-banner bad">
              <strong>{t("buffer.review.twiceTitle")}</strong>
              <span>{t("buffer.review.twiceText")}</span>
              {twin >= 0 && (
                <button type="button" className="btn small" style={{ marginLeft: "auto" }} onClick={() => setIndex(twin)}>
                  {t("buffer.review.otherCopy")}
                </button>
              )}
            </div>
          )}

          {damage.length > 0 && (
            <div className="review-banner bad">
              <strong>{t("buffer.review.damagedTitle")}</strong>
              <ul style={{ margin: 0, paddingLeft: "1.1rem", flexBasis: "100%" }}>
                {damage.map((flag) => (
                  <li key={flag}>{t(DAMAGE[flag] ?? "buffer.flag.unreadableHelp")}</li>
                ))}
              </ul>
              <span>{t("buffer.review.damagedText")}</span>
              <button type="button" className="btn small danger" style={{ marginLeft: "auto" }} onClick={() => setConfirming(true)}>
                {t("buffer.review.discardFile")}
              </button>
            </div>
          )}

          <div className={`review-banner ${banner.kind}`}>
            <strong>{banner.title}</strong>
            <span>{banner.text}</span>
            {keyOff && <Link to="/settings/library">{t("buffer.review.addKey")}</Link>}
          </div>
          {file.error && <span className="bad small">{file.error}</span>}

          {waiting ? (
            <span className="row small muted">
              <span className="spinner" /> {searched ? t("buffer.review.searching") : t("buffer.review.working")}
            </span>
          ) : (
            <>
              {view === "match" && current && (
                <>
                  <div className="row" style={{ alignItems: "flex-start", gap: "1rem", flexWrap: "nowrap" }}>
                    <Cover release={current.release_id} />
                    <div className="stack" style={{ gap: "0.2rem", minWidth: 0 }}>
                      <strong style={{ fontSize: "1.1rem" }}>{file.tags?.title ?? "–"}</strong>
                      <span>{file.tags?.artist ?? "–"}</span>
                      <span className="muted">{releaseLine(current)}</span>
                      <div className="row small" style={{ gap: "1rem" }}>
                        {releases.length > 1 && (
                          <button
                            type="button"
                            className="link-button"
                            aria-expanded={releasesOpen}
                            onClick={() => setReleasesOpen(!releasesOpen)}
                          >
                            {releasesOpen
                              ? t("buffer.review.hideReleases")
                              : t("buffer.review.otherReleases", { count: releases.length })}
                          </button>
                        )}
                        <a href={`https://musicbrainz.org/release/${current.release_id}`} target="_blank" rel="noreferrer">
                          {t("buffer.review.onMusicBrainz")}
                        </a>
                      </div>
                    </div>
                  </div>
                  {releasesOpen && (
                    <AlbumPicker
                      candidates={file.candidates}
                      current={current}
                      kinds={kinds}
                      onKinds={setKinds}
                      busy={busy}
                      onPick={pickRelease}
                    />
                  )}
                  {agree && edited.length === 0 && !tagsOpen ? (
                    <button type="button" className="btn small review-agree" onClick={() => setTagsOpen(true)}>
                      ✓ {t("buffer.review.allAgree")} ▸
                    </button>
                  ) : (
                    <>
                      <div className="review-compare">
                        <span className="head">{t("buffer.tag")}</span>
                        <span className="head">{t("buffer.inFile")}</span>
                        <span className="head">{t("buffer.review.importedWith")}</span>
                        <span className="head" />
                        {FIELDS.map(({ key, field, label, numeric }) => {
                          const own = tagText(file.own_tags?.[key], Boolean(numeric));
                          const value = tagText(file.tags?.[key], Boolean(numeric));
                          const fixed = field in file.tag_edits;
                          const differs = own !== value;
                          return (
                            <FixRow
                              key={`${file.id}-${field}-${value}`}
                              label={t(label)}
                              own={own}
                              value={value}
                              fixed={fixed}
                              differs={differs}
                              disabled={busy}
                              onFix={(text) => fixTag(field, text, value)}
                              onRevert={() => tags.mutate({ reset: [field] })}
                            />
                          );
                        })}
                      </div>
                      <div className="row small" style={{ gap: "1rem" }}>
                        <span className="muted" style={{ flex: 1 }}>
                          {t("buffer.review.fixesHelp")}
                        </span>
                        {edited.length > 0 && (
                          <button
                            type="button"
                            className="link-button warn"
                            disabled={busy}
                            onClick={() => tags.mutate({ reset: edited.map(({ field }) => field) })}
                          >
                            ↺ {t("buffer.review.revertAll")}
                          </button>
                        )}
                      </div>
                    </>
                  )}
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
                        <span>{t(name === "album" ? "buffer.review.albumOptional" : name === "title" ? "buffer.tag.title" : "buffer.tag.artist")}</span>
                        <input
                          className="input"
                          value={query[name]}
                          onChange={(event) => setQuery({ ...query, [name]: event.target.value })}
                        />
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
                  {results.length > 0 && (
                    <>
                      <ul className="review-list">
                        {results.map((candidate) => {
                          const off = Boolean(candidate.length_ms && length && Math.abs(candidate.length_ms - length) > 10_000);
                          return (
                            <li
                              key={`${candidate.recording_id}-${candidate.release_id}-${candidate.disc}-${candidate.track}`}
                              style={off ? { opacity: 0.5 } : undefined}
                            >
                              <CandidateText candidate={candidate} />
                              <span className={`small ${off ? "bad" : "good"}`}>
                                {candidate.length_ms
                                  ? t("buffer.review.lengths", { length: clock(candidate.length_ms), file: clock(length) })
                                  : ""}{" "}
                                · {t("buffer.percent", { score: Math.round(candidate.score * 100) })}
                              </span>
                              <button type="button" className="btn small" disabled={busy} onClick={() => pickRelease(candidate)}>
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
                  {searched && results.length === 0 && (
                    <div className="review-empty">{t("buffer.review.nothing")}</div>
                  )}
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
                    {FIELDS.map(({ field, label, numeric }) => {
                      const name = t(label);
                      const suggested = suggestedField(field);
                      const bad = numeric && badNumber(draft[field]);
                      const common = {
                        "aria-label": name,
                        "aria-invalid": bad || undefined,
                        className: bad ? "input invalid" : "input",
                      };
                      return (
                        <div key={field} className={`field ${SIZES[field] ?? "wide"}`}>
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
              value={String(categoryId)}
              disabled={prepare.isPending || !categories.data}
              onChange={(event) => setCategory(event.target.value)}
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
        <p className="muted">{t("buffer.fill.gone")}</p>
      )}
    </Modal>
  );
}

/** A release's front cover, or nothing when the Cover Art Archive has none. */
function Cover({ release }: Readonly<{ release: string }>) {
  const [shown, setShown] = useState(true);
  useEffect(() => setShown(true), [release]);
  return shown ? (
    <img
      className="cover-thumb"
      src={`https://coverartarchive.org/release/${release}/front-250`}
      alt={t("buffer.cover")}
      loading="lazy"
      onError={() => setShown(false)}
    />
  ) : (
    <div className="cover-thumb review-cover-none" aria-hidden="true">
      <Icon name="image" size={28} />
    </div>
  );
}

/** How to pick among the songs and editions found. */
export function ChooseHelp() {
  return (
    <details className="choose-help small">
      <summary>
        <Icon name="info" size={14} /> {t("buffer.review.choose.title")}
      </summary>
      <ul>
        <li>{t("buffer.review.choose.song")}</li>
        <li>{t("buffer.review.choose.edition")}</li>
        <li>{t("buffer.review.choose.country")}</li>
        <li>{t("buffer.review.choose.later")}</li>
      </ul>
    </details>
  );
}

const KINDS: AlbumKind[] = ["studio", "live", "compilation", "single", "other"];

/** The albums a file or track was found on, to pick another: with the
 * kinds as toggles, as in a search. */
export function AlbumPicker({
  candidates,
  current,
  kinds,
  onKinds,
  busy,
  onPick,
}: Readonly<{
  candidates: Candidate[];
  current: Candidate;
  kinds: AlbumKind[];
  onKinds: (kinds: AlbumKind[]) => void;
  busy: boolean;
  onPick: (candidate: Candidate) => void;
}>) {
  const albums = albumRows(candidates, kinds);
  const album = (candidate: Candidate) => candidate.release_group_id ?? candidate.release_id;
  return (
    <>
      <KindFilter value={kinds} disabled={busy} onChange={onKinds} />
      {albums.widened && <span className="muted small">{t("buffer.review.kinds.widened")}</span>}
      <ul className="review-list">
        {albums.rows.map((candidate) => {
          const chosen = album(candidate) === album(current);
          return (
            <li key={album(candidate)} className={chosen ? "chosen" : undefined}>
              <CandidateText candidate={candidate} />
              <span />
              <button type="button" className="btn small" disabled={chosen || busy} onClick={() => onPick(candidate)}>
                {chosen ? t("buffer.review.picked") : t("buffer.review.pick")}
              </button>
            </li>
          );
        })}
      </ul>
      <ChooseHelp />
    </>
  );
}

/** The kinds of album a search looks on, as toggles: at least one stays on. */
export function KindFilter({
  value,
  disabled,
  onChange,
}: Readonly<{ value: AlbumKind[]; disabled: boolean; onChange: (kinds: AlbumKind[]) => void }>) {
  return (
    <div className="kind-filter" role="group" aria-label={t("buffer.review.kinds.label")}>
      {KINDS.map((kind) => {
        const on = value.includes(kind);
        return (
          <button
            key={kind}
            type="button"
            className="exact-chip"
            aria-pressed={on}
            disabled={disabled || (on && value.length === 1)}
            onClick={() => onChange(on ? value.filter((k) => k !== kind) : KINDS.filter((k) => k === kind || value.includes(k)))}
          >
            {t(`buffer.review.kinds.${kind}`)}
          </button>
        );
      })}
    </div>
  );
}

/** One row per album, in the order given, keeping the kinds wanted (all of
 * them, `widened`, when none is). */
export function albumRows(candidates: Candidate[], kinds: AlbumKind[]): { rows: Candidate[]; widened: boolean } {
  // A search gives its rows already (one per album, or the tracks of the
  // album asked for), in its order and of the kinds it was asked for.
  if (candidates.length > 0 && candidates.every((candidate) => candidate.editions != null)) {
    return { rows: candidates, widened: false };
  }
  // AcoustID gives editions: each album keeps its earliest, counted, studio
  // albums first.
  const grouped = new Map<string, Candidate[]>();
  for (const candidate of candidates) {
    const key = candidate.release_group_id ?? candidate.release_id;
    grouped.set(key, [...(grouped.get(key) ?? []), candidate]);
  }
  const later = (a?: string | null, b?: string | null) => (a ?? "9999").localeCompare(b ?? "9999");
  let rows = [...grouped.values()].map((editions) => {
    const first = [...editions].sort((a, b) => later(a.date, b.date))[0] as Candidate;
    return editions.length === 1
      ? first
      : { ...first, editions: new Set(editions.map((e) => e.release_id)).size, album_date: first.date ?? null };
  });
  const rank = (candidate: Candidate) => (candidate.album_kind ? KINDS.indexOf(candidate.album_kind) : KINDS.length);
  rows = rows.sort((a, b) => rank(a) - rank(b) || later(a.album_date ?? a.date, b.album_date ?? b.date));
  const wanted = rows.filter((candidate) => !candidate.album_kind || kinds.includes(candidate.album_kind));
  return wanted.length > 0 || rows.length === 0 ? { rows: wanted, widened: false } : { rows, widened: true };
}

/** A search result on two lines: the song, then the album it is on (its
 * kind, first year and editions found), linked to MusicBrainz. */
export function CandidateText({ candidate }: Readonly<{ candidate: Candidate }>) {
  const song = [candidate.title, candidate.artist].filter(Boolean).join(" — ");
  const album = candidate.release_group_title ?? candidate.release_title ?? candidate.release_id;
  const year = (candidate.album_date ?? candidate.date)?.slice(0, 4);
  const line = [
    song ? album : null,
    candidate.album_kind ? t(`buffer.review.kinds.${candidate.album_kind}`) : null,
    year,
    candidate.editions && candidate.editions > 1 ? t("buffer.review.editions", { count: candidate.editions }) : null,
    candidate.track ? t("buffer.review.trackNumber", { track: candidate.track }) : null,
  ]
    .filter(Boolean)
    .join(" · ");
  // The edition used for its tags, shown on hover.
  const edition = [
    candidate.release_title,
    candidate.date,
    candidate.country,
    candidate.format,
    candidate.track_count ? t("buffer.review.tracks", { count: candidate.track_count }) : null,
  ]
    .filter(Boolean)
    .join(" · ");
  const href = candidate.release_group_id
    ? `https://musicbrainz.org/release-group/${candidate.release_group_id}`
    : `https://musicbrainz.org/release/${candidate.release_id}`;
  return (
    <span className="candidate-text">
      <span className="ellipsis">{song || album}</span>
      <a
        className="ellipsis muted small edition"
        href={href}
        target="_blank"
        rel="noreferrer"
        title={edition ? t("buffer.review.editionUsed", { edition }) : t("buffer.review.onMusicBrainz")}
      >
        {line || album} ↗
      </a>
    </span>
  );
}

/** One tag of the match: what the file says, what it is imported with (fixed in place). */
export function FixRow({
  label,
  own,
  value,
  fixed,
  differs,
  disabled,
  onFix,
  onRevert,
}: Readonly<{
  label: string;
  own: string;
  value: string;
  fixed: boolean;
  differs: boolean;
  disabled: boolean;
  onFix: (value: string) => void;
  onRevert: () => void;
}>) {
  const [text, setText] = useState(value);
  return (
    <>
      <span className="muted">{label}</span>
      <span className={differs ? "muted struck" : "muted"}>{own || "—"}</span>
      <input
        className={`cell ${fixed ? "fixed" : differs ? "changed" : ""}`}
        aria-label={t("buffer.review.importedAs", { field: label })}
        value={text}
        disabled={disabled}
        onChange={(event) => setText(event.target.value)}
        onBlur={() => onFix(text)}
        onKeyDown={(event) => {
          if (event.key === "Enter") {
            event.currentTarget.blur();
          } else if (event.key === "Escape") {
            event.stopPropagation();
            setText(value);
          }
        }}
      />
      {fixed ? (
        <button
          type="button"
          className="btn small revert"
          title={t("buffer.review.revertHelp")}
          aria-label={t("buffer.review.revert", { field: label })}
          disabled={disabled}
          onClick={onRevert}
        >
          ↺ MB
        </button>
      ) : (
        <span className="small accent">{differs ? t("buffer.review.fromMb") : ""}</span>
      )}
    </>
  );
}
