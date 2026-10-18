// SPDX-License-Identifier: AGPL-3.0-or-later

// Artists: their names, similar artists and the links suggested for review.
// Separation compares artists, not names: names found in the tags are
// linked automatically when they only differ by case, accents or spaces;
// anything else is linked here.

import { keepPreviousData, useQuery } from "@tanstack/react-query";
import { useState } from "react";
import { type Artist, api, done, query, unwrap } from "../api";
import { ArtistPicker } from "../components/ArtistPicker";
import { useDebounced } from "../components/TrackSearch";
import { Card, ConfirmButton, Empty, ErrorView, Field, Icon, Loading, Modal, PageHead } from "../components/ui";
import { count } from "../lib/format";
import { Link, setSearch, useLocation } from "../lib/router";
import { useAction, useCan } from "../lib/session";
import { RestrictionDialog } from "./Restrictions";

const PAGE = 50;

export function Artists() {
  const { search } = useLocation();
  const tab = search.get("tab") === "suggestions" ? "suggestions" : "artists";
  const suggestions = useQuery({
    queryKey: ["artist-suggestions"],
    queryFn: () => unwrap(api.GET("/api/v1/artist-suggestions")),
  });
  const waiting = suggestions.data?.length ?? 0;
  return (
    <>
      <PageHead
        title="Artists"
        description="Separation keeps an artist apart from itself under all its names, and from the artists marked similar."
      />
      <div className="tabs" role="tablist">
        <button
          type="button"
          role="tab"
          aria-selected={tab === "artists"}
          onClick={() => setSearch({ tab: null })}
        >
          Artists
        </button>
        <button
          type="button"
          role="tab"
          aria-selected={tab === "suggestions"}
          onClick={() => setSearch({ tab: "suggestions", artist: null })}
        >
          Suggestions {waiting > 0 && <span className="count">{waiting}</span>}
        </button>
      </div>
      {tab === "artists" ? <ArtistList /> : <Suggestions />}
    </>
  );
}

function ArtistList() {
  const { search } = useLocation();
  const [text, setText] = useState(search.get("q") ?? "");
  const q = useDebounced(text.trim());
  const [offset, setOffset] = useState(0);
  const open = search.get("artist") ? Number(search.get("artist")) : null;
  const artists = useQuery({
    queryKey: ["artists", "list", q, offset],
    queryFn: () =>
      unwrap(api.GET("/api/v1/artists", { params: { query: query({ q, limit: PAGE, offset }) } })),
    placeholderData: keepPreviousData,
  });
  return (
    <Card flush>
      <div className="card-pad">
        <label className="row" style={{ flexWrap: "nowrap" }}>
          <span className="visually-hidden">Search artists</span>
          <Icon name="search" />
          <input
            className="input"
            type="search"
            placeholder="Search any name"
            value={text}
            onChange={(event) => {
              setText(event.target.value);
              setOffset(0);
            }}
          />
        </label>
      </div>
      {artists.isPending && <Loading />}
      {artists.isError && <ErrorView error={artists.error} />}
      {artists.data?.total === 0 && (
        <Empty>{q ? "No artist found." : "No artist yet: they come from the tracks' tags."}</Empty>
      )}
      {artists.data && artists.data.total > 0 && (
        <>
          <div className="table-wrap">
            <table className="table">
              <thead>
                <tr>
                  <th>Artist</th>
                  <th>Also known as</th>
                  <th>Similar</th>
                  <th>Tracks</th>
                </tr>
              </thead>
              <tbody>
                {artists.data.artists.map((artist) => (
                  <tr key={artist.id}>
                    <td className="title-cell">
                      <button type="button" className="link-button" onClick={() => setSearch({ artist: artist.id })}>
                        {artist.name}
                      </button>
                    </td>
                    <td className="small muted">
                      {artist.names
                        .filter((name) => name.name !== artist.name)
                        .map((name) => name.name)
                        .join(", ")}
                    </td>
                    <td className="small">{artist.similar.map((other) => other.name).join(", ")}</td>
                    <td className="num">
                      <Link to={`/library?q=${encodeURIComponent(artist.name)}`}>{artist.tracks}</Link>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
          <div className="pager">
            <span className="muted small">
              {offset + 1}–{Math.min(offset + PAGE, artists.data.total)} of {artists.data.total}
            </span>
            <div className="row">
              <button type="button" className="btn small" disabled={offset === 0} onClick={() => setOffset(offset - PAGE)}>
                Previous
              </button>
              <button
                type="button"
                className="btn small"
                disabled={offset + PAGE >= artists.data.total}
                onClick={() => setOffset(offset + PAGE)}
              >
                Next
              </button>
            </div>
          </div>
        </>
      )}
      {open !== null && <ArtistDialog id={open} onClose={() => setSearch({ artist: null })} />}
    </Card>
  );
}

function ArtistDialog({ id, onClose }: { id: number; onClose: () => void }) {
  const canEdit = useCan("operator");
  const artist = useQuery({
    queryKey: ["artists", id],
    queryFn: () => unwrap(api.GET("/api/v1/artists/{id}", { params: { path: { id } } })),
  });
  const [adding, setAdding] = useState<"similar" | "merge" | null>(null);
  const [moving, setMoving] = useState<number | null>(null);
  const [restricting, setRestricting] = useState(false);
  const [name, setName] = useState<string | null>(null);
  const invalidate = [["artists"], ["artist-suggestions"]];
  const rename = useAction(
    (value: string) => unwrap(api.PATCH("/api/v1/artists/{id}", { params: { path: { id } }, body: { name: value } })),
    { success: "Renamed", invalidate, onSuccess: () => setName(null) },
  );
  const similar = useAction(
    ({ other, linked }: { other: number; linked: boolean }) =>
      done(
        (linked ? api.PUT : api.DELETE)("/api/v1/artists/{id}/similar/{other}", {
          params: { path: { id, other } },
        }),
      ),
    { invalidate, onSuccess: () => setAdding(null) },
  );
  const merge = useAction(
    (other: Artist) =>
      unwrap(api.POST("/api/v1/artists/{id}/merge", { params: { path: { id } }, body: { artist_id: other.id } })),
    { success: (merged) => `Merged into ${merged.name}`, invalidate, onSuccess: () => setAdding(null) },
  );
  const move = useAction(
    ({ nameId, to }: { nameId: number; to: number | null }) =>
      unwrap(
        api.PUT("/api/v1/artist-names/{id}", {
          params: { path: { id: nameId } },
          body: to === null ? {} : { artist_id: to },
        }),
      ),
    {
      success: (destination) => `The name now belongs to ${destination.name}`,
      invalidate,
      onSuccess: () => setMoving(null),
    },
  );

  const data = artist.data;
  return (
    <Modal title={data?.name ?? "Artist"} open wide onClose={onClose}>
      {artist.isPending && <Loading />}
      {artist.isError && <ErrorView error={artist.error} />}
      {data && (
        <div className="stack">
          {name !== null ? (
            <form
              className="row"
              onSubmit={(event) => {
                event.preventDefault();
                rename.mutate(name);
              }}
            >
              <input
                className="input"
                aria-label="Name shown"
                value={name}
                maxLength={512}
                onChange={(event) => setName(event.target.value)}
              />
              <button type="submit" className="btn small primary" disabled={!name.trim()}>
                Save
              </button>
              <button type="button" className="btn small" onClick={() => setName(null)}>
                Cancel
              </button>
            </form>
          ) : (
            <div className="row">
              <span className="muted">
                {count(data.tracks, "track")}
                {data.musicbrainz_id && (
                  <>
                    {" · "}
                    <a href={`https://musicbrainz.org/artist/${data.musicbrainz_id}`} target="_blank" rel="noreferrer">
                      MusicBrainz
                    </a>
                  </>
                )}
              </span>
              <span className="spacer" />
              <Link to={`/restrictions?target=artist&target_id=${data.id}`} className="small">
                Restrictions
              </Link>
              {canEdit && (
                <>
                  <button type="button" className="btn small" onClick={() => setRestricting(true)}>
                    <Icon name="ban" /> Restrict
                  </button>
                  <button type="button" className="btn small" onClick={() => setName(data.name)}>
                    <Icon name="edit" /> Rename
                  </button>
                </>
              )}
            </div>
          )}

          <Field label="Names" hint="Found in the tags. Move a name that belongs to another artist.">
            <ul className="list">
              {data.names.map((artistName) => (
                <li key={artistName.id} className="queue-item">
                  <div className="meta">
                    {artistName.name} {artistName.by_hand && <span className="badge">linked by hand</span>}
                  </div>
                  {canEdit && (
                    <div className="row" style={{ flexWrap: "nowrap" }}>
                      <button type="button" className="btn small" onClick={() => setMoving(artistName.id)}>
                        Move…
                      </button>
                      {data.names.length > 1 && (
                        <button
                          type="button"
                          className="btn small"
                          title="Make it an artist of its own"
                          onClick={() => move.mutate({ nameId: artistName.id, to: null })}
                        >
                          Split off
                        </button>
                      )}
                    </div>
                  )}
                </li>
              ))}
            </ul>
          </Field>
          {moving !== null && (
            <div className="stack">
              <p className="muted small" style={{ margin: 0 }}>
                Move “{data.names.find((n) => n.id === moving)?.name}” to:
              </p>
              <ArtistPicker
                exclude={[data.id]}
                actions={(other) => (
                  <button
                    type="button"
                    className="btn small"
                    onClick={() => move.mutate({ nameId: moving, to: other.id })}
                  >
                    Move here
                  </button>
                )}
              />
              <button type="button" className="link-button" onClick={() => setMoving(null)}>
                Cancel
              </button>
            </div>
          )}

          <Field
            label="Similar artists"
            hint="Separation treats them as this artist: a band and its members' solo projects, a duo and its singers."
          >
            {data.similar.length === 0 ? (
              <span className="muted small">None.</span>
            ) : (
              <ul className="list">
                {data.similar.map((other) => (
                  <li key={other.id} className="queue-item">
                    <div className="meta">
                      <button type="button" className="link-button" onClick={() => setSearch({ artist: other.id })}>
                        {other.name}
                      </button>
                    </div>
                    {canEdit && (
                      <button
                        type="button"
                        className="btn ghost small icon danger"
                        aria-label={`Not similar to ${other.name}`}
                        onClick={() => similar.mutate({ other: other.id, linked: false })}
                      >
                        <Icon name="close" />
                      </button>
                    )}
                  </li>
                ))}
              </ul>
            )}
          </Field>

          {canEdit && adding === null && (
            <div className="row">
              <button type="button" className="btn small" onClick={() => setAdding("similar")}>
                <Icon name="link" /> Add a similar artist
              </button>
              <button type="button" className="btn small" onClick={() => setAdding("merge")}>
                Merge another artist into this one
              </button>
            </div>
          )}
          {adding === "similar" && (
            <ArtistPicker
              exclude={[data.id, ...data.similar.map((s) => s.id)]}
              placeholder="Search the similar artist"
              actions={(other) => (
                <button
                  type="button"
                  className="btn small"
                  onClick={() => similar.mutate({ other: other.id, linked: true })}
                >
                  Similar
                </button>
              )}
            />
          )}
          {adding === "merge" && (
            <ArtistPicker
              exclude={[data.id]}
              placeholder="Search the artist to merge"
              actions={(other) => (
                <ConfirmButton
                  className="btn small"
                  confirm="Merge"
                  question={
                    <p>
                      Merge <strong>{other.name}</strong> into <strong>{data.name}</strong>? Its names, tracks, similar
                      artists and restrictions move to {data.name}, and {other.name} goes. Names can be split off again.
                    </p>
                  }
                  onConfirm={() => merge.mutate(other)}
                >
                  Merge
                </ConfirmButton>
              )}
            />
          )}
          {adding !== null && (
            <button type="button" className="link-button" onClick={() => setAdding(null)}>
              Cancel
            </button>
          )}
          {restricting && (
            <RestrictionDialog
              restriction={null}
              target={{ target: { kind: "artist", id: data.id }, name: data.name }}
              onClose={() => setRestricting(false)}
            />
          )}
        </div>
      )}
    </Modal>
  );
}

function Suggestions() {
  const canEdit = useCan("operator");
  const suggestions = useQuery({
    queryKey: ["artist-suggestions"],
    queryFn: () => unwrap(api.GET("/api/v1/artist-suggestions")),
  });
  const review = useAction(
    ({ id, accept }: { id: number; accept: boolean }) =>
      done(api.POST("/api/v1/artist-suggestions/{id}", { params: { path: { id } }, body: { accept } })),
    { invalidate: [["artist-suggestions"], ["artists"]] },
  );
  const lookUp = useAction(() => unwrap(api.POST("/api/v1/artist-suggestions/musicbrainz")), {
    success: "heron-librarian is looking up MusicBrainz: new suggestions will show here",
    invalidate: [["library-tasks"]],
  });
  return (
    <Card
      flush
      title="Links to review"
      actions={
        canEdit && (
          <button
            type="button"
            className="btn small"
            disabled={lookUp.isPending}
            title="Member of, is person…: for the artists with a MusicBrainz ID"
            onClick={() => lookUp.mutate(undefined)}
          >
            Look up MusicBrainz relationships
          </button>
        )
      }
    >
      {suggestions.isPending && <Loading />}
      {suggestions.isError && <ErrorView error={suggestions.error} />}
      {suggestions.data?.length === 0 && <Empty>Nothing to review.</Empty>}
      {suggestions.data && suggestions.data.length > 0 && (
        <ul className="list">
          {suggestions.data.map((suggestion) => (
            <li key={suggestion.id} className="queue-item">
              <div className="meta">
                <div className="ellipsis">
                  <Link to={`/artists?artist=${suggestion.artist.id}`}>{suggestion.artist.name}</Link> and{" "}
                  <Link to={`/artists?artist=${suggestion.other.id}`}>{suggestion.other.name}</Link>
                </div>
                <div className="muted small ellipsis">
                  {suggestion.source === "musicbrainz" ? "MusicBrainz" : "Names"}: {suggestion.reason}
                </div>
              </div>
              {canEdit && (
                <div className="row" style={{ flexWrap: "nowrap" }}>
                  <button
                    type="button"
                    className="btn small primary"
                    disabled={review.isPending}
                    onClick={() => review.mutate({ id: suggestion.id, accept: true })}
                  >
                    Similar
                  </button>
                  <button
                    type="button"
                    className="btn small"
                    disabled={review.isPending}
                    onClick={() => review.mutate({ id: suggestion.id, accept: false })}
                  >
                    Dismiss
                  </button>
                </div>
              )}
            </li>
          ))}
        </ul>
      )}
    </Card>
  );
}
