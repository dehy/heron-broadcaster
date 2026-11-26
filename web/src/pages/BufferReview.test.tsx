// @vitest-environment jsdom
// SPDX-License-Identifier: AGPL-3.0-or-later

import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { Schemas } from "../api";
import { t } from "../i18n";
import { AlbumPicker, CandidateText, KindFilter, albumRows } from "./BufferReview";

type Candidate = Schemas["ReviewCandidate"];

function edition(release: string, group: string, title: string, date: string, kind?: Schemas["AlbumKind"]): Candidate {
  return {
    score: 0.97,
    recording_id: "rec",
    title: "Hit",
    artist: "Someone",
    release_id: release,
    release_title: title,
    release_group_id: group,
    release_group_title: title,
    date,
    country: "GB",
    disc: 1,
    track: 3,
    ...(kind ? { album_kind: kind } : {}),
  };
}

// What AcoustID gives: editions, best first, not grouped.
const FOUND = [
  edition("best", "g-best", "Best Of", "1990", "compilation"),
  edition("late", "g-album", "Album", "2005", "studio"),
  edition("first", "g-album", "Album", "1980-05-01", "studio"),
  edition("live", "g-live", "Live", "1985", "live"),
];

describe("albumRows", () => {
  it("keeps one edition per album, the earliest, studio albums first", () => {
    const { rows, widened } = albumRows(FOUND, ["studio", "live", "compilation", "single", "other"]);
    expect(rows.map((row) => [row.release_id, row.editions ?? 1])).toEqual([
      ["first", 2],
      ["live", 1],
      ["best", 1],
    ]);
    expect(widened).toBe(false);
  });

  it("keeps the kinds asked, or every kind when none is there", () => {
    expect(albumRows(FOUND, ["live"]).rows.map((row) => row.release_id)).toEqual(["live"]);
    const none = albumRows(FOUND, ["single"]);
    expect(none.widened).toBe(true);
    expect(none.rows).toHaveLength(3);
  });

  it("keeps a search's order: one row per album already", () => {
    const searched = [
      { ...edition("b", "g-b", "B", "1990", "studio"), editions: 1 },
      { ...edition("a", "g-a", "A", "1970", "studio"), editions: 3 },
    ];
    expect(albumRows(searched, ["studio"]).rows.map((row) => row.release_id)).toEqual(["b", "a"]);
  });

  it("keeps the tracks of an album asked for, of any kind", () => {
    const tracklist = [1, 2].map((track) => ({
      ...edition(`r`, "g-best", "Best Of", "1990", "compilation"),
      recording_id: `rec-${track}`,
      track,
      editions: 1,
    }));
    const { rows, widened } = albumRows(tracklist, ["studio"]);
    expect(rows.map((row) => row.recording_id)).toEqual(["rec-1", "rec-2"]);
    expect(widened).toBe(false);
  });
});

describe("the album rows", () => {
  it("shows the song, then the album with its kind, year and editions", () => {
    render(<CandidateText candidate={{ ...edition("first", "g-album", "Album", "1980-05-01", "studio"), editions: 4 }} />);
    expect(screen.getByText("Hit — Someone")).toBeTruthy();
    const album = screen.getByRole("link");
    expect(album.textContent).toContain(`Album · ${t("buffer.review.kinds.studio")} · 1980`);
    expect(album.textContent).toContain(t("buffer.review.editions", { count: 4 }));
    expect(album.getAttribute("href")).toBe("https://musicbrainz.org/release-group/g-album");
  });

  it("toggles the kinds, one always on", () => {
    const onChange = vi.fn();
    render(<KindFilter value={["studio"]} disabled={false} onChange={onChange} />);
    const studio = screen.getByRole("button", { name: t("buffer.review.kinds.studio") });
    expect(studio.getAttribute("aria-pressed")).toBe("true");
    expect((studio as HTMLButtonElement).disabled).toBe(true);
    fireEvent.click(screen.getByRole("button", { name: t("buffer.review.kinds.live") }));
    expect(onChange).toHaveBeenCalledWith(["studio", "live"]);
  });

  it("picks another album, not the one in use", () => {
    const onPick = vi.fn();
    render(
      <AlbumPicker
        candidates={FOUND}
        current={FOUND[1] as Candidate}
        kinds={["studio", "compilation"]}
        onKinds={() => {}}
        busy={false}
        onPick={onPick}
      />,
    );
    const picked = screen.getByRole("button", { name: t("buffer.review.picked") });
    expect((picked as HTMLButtonElement).disabled).toBe(true);
    fireEvent.click(screen.getByRole("button", { name: t("buffer.review.pick") }));
    expect(onPick).toHaveBeenCalledWith(expect.objectContaining({ release_id: "best" }));
  });
});
