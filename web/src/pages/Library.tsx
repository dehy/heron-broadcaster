// SPDX-License-Identifier: AGPL-3.0-or-later

// The library: search, filters, sorting, pages, and queuing.

import { keepPreviousData, useQuery } from "@tanstack/react-query";
import { useEffect, useState } from "react";
import {
  type TrackFilter,
  type TrackSort,
  type TrackSummary,
  api,
  fetchLibraryStatus,
  query,
  unwrap,
} from "../api";
import { useDebounced } from "../components/TrackSearch";
import { Card, Empty, ErrorView, Icon, Loading, PageHead } from "../components/ui";
import { ago, duration } from "../lib/format";
import { Link, setSearch, useLocation } from "../lib/router";
import { useAction, useCan } from "../lib/session";

const PAGE = 50;

const FILTERS: { value: TrackFilter | ""; label: string }[] = [
  { value: "", label: "All tracks" },
  { value: "playable", label: "Playable" },
  { value: "unanalyzed", label: "Not analyzed" },
  { value: "silent", label: "Silent" },
  { value: "missing", label: "Missing files" },
  { value: "disabled", label: "Disabled" },
  { value: "needs_review", label: "Needs review" },
];

const SORTS: { value: TrackSort; label: string }[] = [
  { value: "artist", label: "Artist" },
  { value: "title", label: "Title" },
  { value: "album", label: "Album" },
  { value: "added", label: "Recently added" },
  { value: "last_played", label: "Least recently played" },
];

export function Library() {
  const { search } = useLocation();
  const canQueue = useCan("operator");
  const filter = (search.get("filter") ?? "") as TrackFilter | "";
  const sort = (search.get("sort") ?? "artist") as TrackSort;
  const category = search.get("category") ?? "";
  const page = Math.max(0, Number(search.get("page") ?? 0) || 0);
  const [text, setText] = useState(search.get("q") ?? "");
  const q = useDebounced(text.trim(), 300);
  const [selected, setSelected] = useState<Set<number>>(new Set());

  useEffect(() => {
    if (q !== (search.get("q") ?? "")) {
      setSearch({ q, page: undefined });
    }
  }, [q, search]);

  const categories = useQuery({
    queryKey: ["categories"],
    queryFn: () => unwrap(api.GET("/api/v1/categories")),
  });
  const status = useQuery({
    queryKey: ["library-status"],
    queryFn: ({ signal }) => fetchLibraryStatus(signal),
    refetchInterval: 10_000,
  });
  const tracks = useQuery({
    queryKey: ["tracks", { q, filter, sort, category, page }],
    queryFn: ({ signal }) =>
      unwrap(
        api.GET("/api/v1/tracks", {
          params: {
            query: query({
              q,
              filter: filter || undefined,
              sort,
              category_id: category ? Number(category) : undefined,
              limit: PAGE,
              offset: page * PAGE,
            }),
          },
          signal,
        }),
      ),
    placeholderData: keepPreviousData,
  });

  const enqueue = useAction(
    ({ ids, next }: { ids: number[]; next: boolean }) =>
      unwrap(
        api.POST("/api/v1/queue", { body: next ? { track_ids: ids, position: 0 } : { track_ids: ids } }),
      ),
    {
      success: (result) =>
        result.entry_ids.length === 1 ? "Queued" : `${result.entry_ids.length} tracks queued`,
      onSuccess: () => setSelected(new Set()),
    },
  );

  const items = tracks.data?.items ?? [];
  const total = tracks.data?.total ?? 0;
  const pages = Math.max(1, Math.ceil(total / PAGE));
  const toggle = (id: number) =>
    setSelected((current) => {
      const next = new Set(current);
      if (next.has(id)) {
        next.delete(id);
      } else {
        next.add(id);
      }
      return next;
    });
  const allSelected = items.length > 0 && items.every((track) => selected.has(track.id));

  return (
    <>
      <PageHead title="Tracks" description={<LibrarySummary status={status.data} />} />
      <Card flush>
        <div className="card-head">
          <div className="row" style={{ flex: 1 }}>
            <label className="row" style={{ flex: "1 1 16rem", flexWrap: "nowrap" }}>
              <Icon name="search" />
              <span className="visually-hidden">Search</span>
              <input
                className="input"
                type="search"
                placeholder="Title, artist, album or file"
                value={text}
                onChange={(event) => setText(event.target.value)}
              />
            </label>
            <label>
              <span className="visually-hidden">Show</span>
              <select
                className="input"
                value={filter}
                onChange={(event) => setSearch({ filter: event.target.value, page: undefined })}
              >
                {FILTERS.map((option) => (
                  <option key={option.value} value={option.value}>
                    {option.label}
                  </option>
                ))}
              </select>
            </label>
            <label>
              <span className="visually-hidden">Category</span>
              <select
                className="input"
                value={category}
                onChange={(event) => setSearch({ category: event.target.value, page: undefined })}
              >
                <option value="">Any category</option>
                {categories.data?.map((c) => (
                  <option key={c.id} value={c.id}>
                    {c.name}
                  </option>
                ))}
              </select>
            </label>
            <label>
              <span className="visually-hidden">Sort by</span>
              <select
                className="input"
                value={sort}
                onChange={(event) => setSearch({ sort: event.target.value, page: undefined })}
              >
                {SORTS.map((option) => (
                  <option key={option.value} value={option.value}>
                    Sort: {option.label}
                  </option>
                ))}
              </select>
            </label>
          </div>
        </div>
        {canQueue && selected.size > 0 && (
          <div className="card-head" style={{ background: "var(--accent-soft)" }}>
            <span>{selected.size} selected</span>
            <div className="row">
              <button
                type="button"
                className="btn small"
                onClick={() => enqueue.mutate({ ids: [...selected], next: true })}
              >
                <Icon name="next" size={14} /> Play next
              </button>
              <button
                type="button"
                className="btn small"
                onClick={() => enqueue.mutate({ ids: [...selected], next: false })}
              >
                <Icon name="plus" size={14} /> Add to the queue
              </button>
              <button type="button" className="btn small ghost" onClick={() => setSelected(new Set())}>
                Clear
              </button>
            </div>
          </div>
        )}
        {tracks.isPending && <Loading />}
        {tracks.isError && (
          <div className="card-body">
            <ErrorView error={tracks.error} />
          </div>
        )}
        {tracks.data && items.length === 0 && <Empty>No track matches.</Empty>}
        {items.length > 0 && (
          <div className="table-wrap">
            <table className="table">
              <thead>
                <tr>
                  {canQueue && (
                    <th style={{ width: "2rem" }}>
                      <input
                        type="checkbox"
                        aria-label="Select the whole page"
                        checked={allSelected}
                        onChange={() =>
                          setSelected(allSelected ? new Set() : new Set(items.map((track) => track.id)))
                        }
                      />
                    </th>
                  )}
                  <th>Title</th>
                  <th className="hide-narrow">Artist</th>
                  <th className="hide-narrow">Album</th>
                  <th>Length</th>
                  <th className="hide-narrow">Last played</th>
                  <th>
                    <span className="visually-hidden">Actions</span>
                  </th>
                </tr>
              </thead>
              <tbody>
                {items.map((track) => (
                  <Row
                    key={track.id}
                    track={track}
                    canQueue={canQueue}
                    selected={selected.has(track.id)}
                    onToggle={() => toggle(track.id)}
                    onQueue={(next) => enqueue.mutate({ ids: [track.id], next })}
                    onArtist={setText}
                  />
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
              Previous
            </button>
            <span className="muted small num">
              Page {page + 1} of {pages} · {total.toLocaleString()} tracks
            </span>
            <button
              type="button"
              className="btn small"
              disabled={page + 1 >= pages}
              onClick={() => setSearch({ page: page + 1 })}
            >
              Next
            </button>
          </div>
        )}
      </Card>
    </>
  );
}

function LibrarySummary({ status }: { status: Awaited<ReturnType<typeof fetchLibraryStatus>> | undefined }) {
  if (!status) {
    return null;
  }
  const queue = status.analysis_queue;
  const analyzing = queue.pending + queue.running;
  return (
    <>
      {status.tracks.toLocaleString()} tracks, {status.analyzed.toLocaleString()} analyzed
      {analyzing > 0 && <>, {analyzing.toLocaleString()} being analyzed</>}
      {status.missing > 0 && (
        <>
          , <Link to="/library?filter=missing">{status.missing} missing</Link>
        </>
      )}
      {status.silent > 0 && (
        <>
          , <Link to="/library?filter=silent">{status.silent} silent</Link>
        </>
      )}
      {queue.failed > 0 && <>, {queue.failed} unreadable</>}
      {status.last_scan && <>. Last scan {ago(status.last_scan.finished_at_unix * 1000)}</>}.
    </>
  );
}

function TrackState({ track }: { track: TrackSummary }) {
  if (track.missing) {
    return <span className="badge bad">Missing</span>;
  }
  if (!track.enabled) {
    return <span className="badge">Disabled</span>;
  }
  if (!track.analyzed) {
    return <span className="badge warn">Not analyzed</span>;
  }
  if (track.silent) {
    return <span className="badge bad">Silent</span>;
  }
  return null;
}

function Row({
  track,
  canQueue,
  selected,
  onToggle,
  onQueue,
  onArtist,
}: {
  track: TrackSummary;
  canQueue: boolean;
  selected: boolean;
  onToggle: () => void;
  onQueue: (next: boolean) => void;
  onArtist: (artist: string) => void;
}) {
  const playable = !track.missing && !track.silent;
  const file = track.path.split("/").pop() ?? track.path;
  return (
    <tr>
      {canQueue && (
        <td>
          <input
            type="checkbox"
            aria-label={`Select ${track.title ?? file}`}
            checked={selected}
            onChange={onToggle}
            disabled={!playable}
          />
        </td>
      )}
      <td className="title-cell">
        <div className="row" style={{ flexWrap: "nowrap" }}>
          <Link to={`/library/${track.id}`} className="ellipsis">
            {track.title ?? file}
          </Link>
          <TrackState track={track} />
        </div>
      </td>
      <td className="hide-narrow ellipsis" style={{ maxWidth: "14rem" }}>
        {track.artist ? (
          <button
            type="button"
            className="link-button"
            title={`Search ${track.artist}`}
            onClick={() => onArtist(track.artist ?? "")}
          >
            {track.artist}
          </button>
        ) : (
          <span className="muted">–</span>
        )}
      </td>
      <td className="hide-narrow ellipsis" style={{ maxWidth: "14rem" }}>
        {track.album ?? <span className="muted">–</span>}
      </td>
      <td className="num">{duration(track.duration_ms)}</td>
      <td className="hide-narrow muted small">{ago(track.last_played_at_unix_ms)}</td>
      <td className="actions">
        {canQueue && playable && (
          <>
            <button
              type="button"
              className="btn ghost small icon"
              title="Play next"
              aria-label={`Play ${track.title ?? file} next`}
              onClick={() => onQueue(true)}
            >
              <Icon name="next" />
            </button>
            <button
              type="button"
              className="btn ghost small icon"
              title="Add to the queue"
              aria-label={`Add ${track.title ?? file} to the queue`}
              onClick={() => onQueue(false)}
            >
              <Icon name="plus" />
            </button>
          </>
        )}
      </td>
    </tr>
  );
}
