// SPDX-License-Identifier: AGPL-3.0-or-later

// Searching the library for a track to queue, play on the FX player or
// preview a transition with.

import { keepPreviousData, useQuery } from "@tanstack/react-query";
import { type ReactNode, useEffect, useState } from "react";
import { type TrackSummary, api, query, unwrap } from "../api";
import { duration, trackName } from "../lib/format";
import { ErrorView, Icon } from "./ui";

/** `value`, once it stopped changing for `ms`. */
export function useDebounced<T>(value: T, ms = 250): T {
  const [settled, setSettled] = useState(value);
  useEffect(() => {
    const timer = setTimeout(() => setSettled(value), ms);
    return () => clearTimeout(timer);
  }, [value, ms]);
  return settled;
}

export function TrackSearch({
  actions,
  placeholder = "Search title, artist, album or file",
  playableOnly = true,
  limit = 8,
}: {
  /** Buttons for each result. */
  actions: (track: TrackSummary) => ReactNode;
  placeholder?: string;
  playableOnly?: boolean;
  limit?: number;
}) {
  const [text, setText] = useState("");
  const q = useDebounced(text.trim());
  const results = useQuery({
    queryKey: ["tracks", "search", q, playableOnly, limit],
    queryFn: ({ signal }) =>
      unwrap(
        api.GET("/api/v1/tracks", {
          params: {
            query: query({
              q,
              filter: playableOnly ? ("playable" as const) : undefined,
              sort: q ? undefined : ("last_played" as const),
              limit,
            }),
          },
          signal,
        }),
      ),
    placeholderData: keepPreviousData,
  });
  return (
    <div className="stack">
      <label className="row" style={{ flexWrap: "nowrap" }}>
        <span className="visually-hidden">Search tracks</span>
        <Icon name="search" />
        <input
          className="input"
          type="search"
          placeholder={placeholder}
          value={text}
          onChange={(event) => setText(event.target.value)}
        />
      </label>
      {results.isError && <ErrorView error={results.error} />}
      {results.data && (
        <ul className="list">
          {results.data.items.length === 0 && <li className="empty">No playable track found.</li>}
          {results.data.items.map((track) => (
            <li key={track.id} className="queue-item">
              <div className="meta">
                <div className="ellipsis">{trackName(track)}</div>
                <div className="muted small ellipsis">
                  {track.album ?? track.path} · {duration(track.duration_ms)}
                </div>
              </div>
              <div className="row" style={{ flexWrap: "nowrap" }}>
                {actions(track)}
              </div>
            </li>
          ))}
        </ul>
      )}
      {!q && results.data && results.data.items.length > 0 && (
        <p className="muted small" style={{ margin: 0 }}>
          Showing the least recently played tracks. Type to search.
        </p>
      )}
    </div>
  );
}
