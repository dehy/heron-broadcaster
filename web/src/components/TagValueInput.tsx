// SPDX-License-Identifier: AGPL-3.0-or-later

// A tag field that proposes the values the library already has while it is
// typed (artists, album artists, albums): `GET /library/tag-values`.

import { keepPreviousData, useQuery } from "@tanstack/react-query";
import { type InputHTMLAttributes, type KeyboardEvent, useEffect, useId, useRef, useState } from "react";
import { type Schemas, api, unwrap } from "../api";
import { t } from "../i18n";

/** `field` of `GET /library/tag-values`. */
export type TagValueField = "artist" | "album_artist" | "album";

/** A value the library has, with how many tracks (and for an album, who by and when). */
export type TagValue = Schemas["TagValue"];

/** The tag fields that have proposals, by their name in the tag editors. */
export function suggestedField(field: string): TagValueField | undefined {
  return field === "artist" || field === "album_artist" || field === "album" ? field : undefined;
}

/** How long typing must pause before the library is asked. */
const PAUSE_MS = 200;

/** The library's values for what is typed, the most used first. */
export function useTagValues(
  field: TagValueField | undefined,
  typed: string,
  artist?: string | null | undefined,
): TagValue[] {
  // Asked once something is typed, not for the value a field starts with.
  const initial = useRef<string | null>(typed);
  const [asked, setAsked] = useState("");
  useEffect(() => {
    if (typed === initial.current) {
      return undefined;
    }
    initial.current = null;
    const timer = setTimeout(() => setAsked(typed.trim()), PAUSE_MS);
    return () => clearTimeout(timer);
  }, [typed]);
  const byArtist = field === "album" ? artist?.trim() || undefined : undefined;
  const values = useQuery({
    queryKey: ["tag-values", field, asked, byArtist ?? null],
    queryFn: () =>
      unwrap(
        api.GET("/api/v1/library/tag-values", {
          params: { query: { field: field ?? "artist", q: asked, ...(byArtist ? { artist: byArtist } : {}) } },
        }),
      ),
    enabled: field !== undefined && asked.length > 0 && asked.length <= 100,
    staleTime: 60_000,
    placeholderData: keepPreviousData,
  });
  if (field === undefined || !typed.trim()) {
    return [];
  }
  // A name typed exactly is not proposed again; an album still is, for
  // who it is by and its year.
  return (values.data?.values ?? []).filter((v) => field === "album" || v.value !== typed);
}

/** A letter without its accent, in lower case: how the library matches. */
function fold(letter: string): string {
  return letter.normalize("NFD").replace(/\p{M}/gu, "").toLowerCase();
}

/** `value` cut around what is typed, where it matches at the start of a word. */
export function matched(value: string, typed: string): [string, string, string] {
  const letters = [...value];
  const wanted = [...typed.trim()].map(fold).join("");
  if (!wanted) {
    return [value, "", ""];
  }
  for (let start = 0; start < letters.length; start += 1) {
    if (start > 0 && letters[start - 1] !== " ") {
      continue;
    }
    let text = "";
    let end = start;
    while (end < letters.length && text.length < wanted.length) {
      text += fold(letters[end] ?? "");
      end += 1;
    }
    if (text === wanted) {
      return [letters.slice(0, start).join(""), letters.slice(start, end).join(""), letters.slice(end).join("")];
    }
  }
  return [value, "", ""];
}

/** What a proposal says besides its name. */
function detail(value: TagValue): string {
  return [value.album_artist, value.year, t("common.tracks", { count: value.tracks })]
    .filter((part) => part !== null && part !== undefined && part !== "")
    .join(" · ");
}

/**
 * A text input with the library's values proposed under it, as a list the
 * arrows walk through and Enter picks. `artist` puts that artist's albums
 * first; `onPick` is told the whole value picked (an album's artist and year).
 */
export function TagValueInput({
  field,
  value,
  onValue,
  onPick,
  artist,
  onKeyDown,
  onBlur,
  ...props
}: Readonly<
  {
    field: TagValueField;
    value: string;
    onValue: (value: string) => void;
    onPick?: ((value: TagValue) => void) | undefined;
    artist?: string | null | undefined;
  } & Omit<InputHTMLAttributes<HTMLInputElement>, "value" | "onChange" | "list" | "type" | "role">
>) {
  const list = useId();
  const values = useTagValues(field, value, artist);
  const [open, setOpen] = useState(false);
  const [active, setActive] = useState(-1);
  useEffect(() => setActive(-1), [values.length, values[0]?.value]);
  const shown = open && values.length > 0;
  const pick = (picked: TagValue) => {
    onValue(picked.value);
    onPick?.(picked);
    setOpen(false);
  };
  const keys = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      if (!shown) {
        setOpen(true);
        return;
      }
      const step = event.key === "ArrowDown" ? 1 : -1;
      setActive((current) => Math.min(values.length - 1, Math.max(-1, current + step)));
      return;
    }
    const picked = values[active];
    if (shown && event.key === "Enter" && !event.ctrlKey && !event.metaKey && picked) {
      event.preventDefault();
      event.stopPropagation();
      pick(picked);
      return;
    }
    if (shown && event.key === "Escape") {
      // The list closes; the dialog around stays.
      event.preventDefault();
      event.stopPropagation();
      setOpen(false);
      return;
    }
    onKeyDown?.(event);
  };
  return (
    <div className="combo">
      <input
        className="input"
        type="text"
        autoComplete="off"
        {...props}
        role="combobox"
        aria-autocomplete="list"
        aria-expanded={shown}
        aria-controls={list}
        aria-activedescendant={shown && active >= 0 ? `${list}-${active}` : undefined}
        value={value}
        onChange={(event) => {
          onValue(event.target.value);
          setOpen(true);
        }}
        onKeyDown={keys}
        onBlur={(event) => {
          setOpen(false);
          onBlur?.(event);
        }}
      />
      <ul className="combo-list" role="listbox" id={list} hidden={!shown}>
        {values.map((option, index) => {
          const [before, match, after] = matched(option.value, value);
          return (
            <li
              key={`${option.value}\u0000${option.album_artist ?? ""}`}
              id={`${list}-${index}`}
              role="option"
              aria-selected={index === active}
              onMouseDown={(event) => event.preventDefault()}
              onClick={() => pick(option)}
            >
              <span className="ellipsis">
                {before}
                {match && <mark>{match}</mark>}
                {after}
              </span>
              <span className="combo-detail">{detail(option)}</span>
            </li>
          );
        })}
      </ul>
    </div>
  );
}
