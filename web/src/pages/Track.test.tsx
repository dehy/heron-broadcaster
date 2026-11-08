// @vitest-environment jsdom
// SPDX-License-Identifier: AGPL-3.0-or-later

import { fireEvent, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { t } from "../i18n";
import { chooseStation } from "../lib/station";
import { answer, browserStubs, lenient, renderApp, sent, user } from "../test/render";
import { Track, annotations, effectiveCues, mixOf as realMixOf } from "./Track";

browserStubs();

const TRACK = {
  id: 7,
  title: "Blue Monday",
  artist: "New Order",
  album: "Power, Corruption & Lies",
  album_artist: null,
  genre: "Synth-pop",
  year: 1983,
  path: "New Order/Blue Monday.mp3",
  duration_ms: 240_000,
  enabled: true,
  missing: false,
  silent: false,
  analyzed: true,
  play_count: 3,
  added_at_unix_ms: 1_700_000_000_000,
  deleted_at_unix_ms: null,
  labels: [],
  categories: [],
  tag_overrides: [],
  overrides: { crossfade: {} },
  markers: { intro_ms: 8_000, intro2_ms: null, hook_in_ms: 60_000, hook_out_ms: 80_000, next_soft_ms: null, next_hard_ms: null },
  analysis: {
    cue_in_ms: 1_000,
    cue_out_ms: 230_000,
    fade_out_start_ms: 220_000,
    integrated_lufs: -12.5,
    true_peak_db: -0.8,
    analyzed_at_unix_ms: 1_700_000_100_000,
    has_internal_silence: false,
  },
  playout: { gain_db: -1.5 },
  file: { size: 5_000_000, channels: 2, sample_rate: 44_100, missing_since_unix_ms: null, year_tag: 1983, track_number: 1 },
  identification: null,
  musicbrainz: null,
  plays: [],
  programming: { mood: null, sound_code: null, era: null },
};

const SETTINGS = {
  crossfade: {
    mode: "auto",
    fixed_ms: 2000,
    min_track_ms: 10_000,
    fade_in: { enabled: false, time_ms: 1000, curve: "linear" },
    fade_out: { enabled: true, time_ms: 3000, curve: "linear" },
  },
};

function page(track: Record<string, unknown> = TRACK, role: "viewer" | "operator" = "operator") {
  answer(
    lenient({
      "GET /api/v1/tracks/{id}": track,
      "GET /api/v1/stations/{station_id}/settings/playout": SETTINGS,
      "GET /api/v1/tracks/{id}/waveform": { interval_ms: 100, floor_db: -60, peaks: "" },
      "GET /api/v1/settings/programming": { moods: ["calm"], sound_codes: ["a"] },
      "POST /api/v1/stations/{station_id}/queue": undefined,
    }),
  );
  chooseStation(1);
  renderApp(<Track id={7} />, user(role));
}

describe("Track", () => {
  it("shows the track, its cue points and its details", async () => {
    page();
    expect(await screen.findByText("Blue Monday")).toBeTruthy();
    expect(screen.getByText(t("track.waveform"))).toBeTruthy();
    expect(screen.getAllByText(t("track.cueIn")).length).toBeGreaterThan(0);
    expect(screen.getByText(t("track.stereo"), { exact: false })).toBeTruthy();
  });

  it("offers the actions to an operator, not to a viewer", async () => {
    page();
    expect(await screen.findByText(t("track.editTags"))).toBeTruthy();
  });

  it("hides the actions from a viewer", async () => {
    page(TRACK, "viewer");
    await screen.findByText("Blue Monday");
    expect(screen.queryByText(t("track.editTags"))).toBeNull();
  });

  it("warns about a missing, silent or disabled track", async () => {
    page({ ...TRACK, missing: true, silent: true, enabled: false, tag_overrides: ["title"] });
    expect(await screen.findByText(t("track.silent"))).toBeTruthy();
    expect(screen.getByText(t("track.disabled"))).toBeTruthy();
    expect(screen.queryByText(t("track.queue"))).toBeNull();
  });

  it("asks to review a track that needs it", async () => {
    page({ ...TRACK, identification: { status: "needs_review", reason: "two releases" } });
    expect(await screen.findByText(t("track.reviewIt"))).toBeTruthy();
  });

  it("queues the track next", async () => {
    page();
    fireEvent.click(await screen.findByText(t("queue.playNext")));
    await waitFor(() => expect(sent("POST /api/v1/stations/{station_id}/queue")).toHaveLength(1));
  });

  it("disables the track", async () => {
    answer(lenient({ "GET /api/v1/tracks/{id}": TRACK, "PATCH /api/v1/tracks/{id}": TRACK }));
    chooseStation(1);
    renderApp(<Track id={7} />, user("operator"));
    fireEvent.click(await screen.findByText(t("track.disable")));
    await waitFor(() => expect(sent("PATCH /api/v1/tracks/{id}")).toHaveLength(1));
    expect(sent("PATCH /api/v1/tracks/{id}")[0]?.options).toMatchObject({ body: { enabled: false } });
  });

  it("writes the tags the file lacks", async () => {
    answer(
      lenient({
        "GET /api/v1/tracks/{id}": { ...TRACK, tag_overrides: ["title"] },
        "POST /api/v1/tracks/tag-overrides/clear": undefined,
      }),
    );
    chooseStation(1);
    renderApp(<Track id={7} />, user("operator"));
    fireEvent.click(await screen.findByText(t("track.fileTags")));
    await waitFor(() => expect(sent("POST /api/v1/tracks/tag-overrides/clear")).toHaveLength(1));
  });

  it.each([
    ["done", { skipped: [] }, undefined, t("track.deleted")],
    ["done", { skipped: [{ reason: "in use" }] }, undefined, t("track.notDeleted", { reason: "in use" })],
    ["failed", null, "disk full", "disk full"],
  ])("tells how deleting the track ended (%s)", async (status, report, error, message) => {
    answer(
      lenient({
        "GET /api/v1/tracks/{id}": TRACK,
        "POST /api/v1/tracks/delete": { id: 9 },
        "GET /api/v1/library/tasks/{id}": { id: 9, status, report, error },
      }),
    );
    chooseStation(1);
    renderApp(<Track id={7} />, user("operator"));
    fireEvent.click(await screen.findByText(t("library.delete")));
    fireEvent.click((await screen.findAllByText(t("library.delete"))).at(-1) as HTMLElement);
    await waitFor(() => expect(sent("POST /api/v1/tracks/delete")).toHaveLength(1));
    expect(await screen.findByText(message)).toBeTruthy();
  });

  it("loops the hook", async () => {
    page();
    fireEvent.click(await screen.findByText(t("track.loopHook")));
    expect(await screen.findByText(t("track.stopHook"))).toBeTruthy();
    // jsdom plays no audio: the button stays until the element says it paused.
    fireEvent.click(screen.getByText(t("track.stopHook")));
  });

  it("follows the audio while it plays, looping the hook", async () => {
    page();
    fireEvent.click(await screen.findByText(t("track.loopHook")));
    const audio = document.querySelector("audio") as HTMLAudioElement;
    // The hook runs 60 s to 80 s: past its end, the audio goes back to its start.
    audio.currentTime = 81;
    fireEvent.play(audio);
    await waitFor(() => expect(audio.currentTime).toBeLessThan(81));
    fireEvent.pause(audio);
    expect(await screen.findByText(t("track.listen"))).toBeTruthy();
  });

  it("pauses the audio from the play button", async () => {
    page();
    const audio = (await screen.findByText(t("track.listen")).then(() => document.querySelector("audio"))) as HTMLAudioElement;
    fireEvent.play(audio);
    fireEvent.click(await screen.findByText(t("dashboard.pause")));
    fireEvent.ended(audio);
    expect(await screen.findByText(t("track.listen"))).toBeTruthy();
  });

  it("zooms on the intro", async () => {
    page();
    fireEvent.click(await screen.findByText(t("track.intro")));
    expect(screen.getByText(t("track.intro"))).toBeTruthy();
  });
});

describe("effectiveCues", () => {
  const track = { duration_ms: 100_000, analysis: { cue_in_ms: 2_000, cue_out_ms: 90_000 } } as never;

  it("plays from the analysis' cue points", () => {
    expect(effectiveCues(track, { crossfade: {} } as never)).toEqual({ length: 100_000, cueIn: 2_000, cueOut: 90_000 });
  });

  it("lets the overrides win, within the track", () => {
    const cues = effectiveCues(track, { cue_in_ms: 5_000, cue_out_ms: 500_000, crossfade: {} } as never);
    expect(cues).toEqual({ length: 100_000, cueIn: 5_000, cueOut: 100_000 });
  });

  it("plays the whole track without the gap killer", () => {
    const cues = effectiveCues(track, { gap_killer_enabled: false, crossfade: {} } as never);
    expect(cues).toEqual({ length: 100_000, cueIn: 0, cueOut: 100_000 });
  });
});

describe("mixOf", () => {
  const settings = (changes: Record<string, unknown> = {}) =>
    ({
      crossfade: {
        mode: "auto",
        fixed_ms: 2_000,
        min_track_ms: 10_000,
        fade_in: { enabled: true, time_ms: 1_000, curve: "linear" },
        fade_out: { enabled: true, time_ms: 3_000, curve: "linear" },
        ...changes,
      },
    }) as never;
  const track = { analysis: { fade_out_start_ms: 220_000 } };
  const mixOf = (trk: unknown, draft: unknown, config: unknown, cueIn: number, cueOut: number) =>
    realMixOf(trk as never, draft as never, config as never, cueIn, cueOut);

  it("mixes where the analysis says", () => {
    const mix = mixOf(track, { crossfade: {} }, settings(), 1_000, 230_000);
    expect(mix).toMatchObject({ shortTrack: false, crossfades: true, mixPoint: 220_000 });
    expect(mix.fadeOut?.enabled).toBe(true);
  });

  it("ignores an analysis point outside the cues", () => {
    expect(mixOf({ analysis: { fade_out_start_ms: 500 } }, { crossfade: {} }, settings(), 1_000, 230_000).mixPoint).toBeNull();
    expect(mixOf({ analysis: null }, { crossfade: {} }, settings(), 1_000, 230_000).mixPoint).toBeNull();
  });

  it("mixes a fixed time before the cue-out", () => {
    expect(mixOf(track, { crossfade: {} }, settings({ mode: "fixed", fixed_ms: 4_000 }), 1_000, 230_000).mixPoint).toBe(226_000);
    expect(mixOf(track, { crossfade: { mode: "fixed", fixed_ms: 400_000 } }, settings(), 1_000, 230_000).mixPoint).toBe(1_000);
  });

  it("does not mix when the track says so", () => {
    const mix = mixOf(track, { crossfade: { crossfade: false } }, settings(), 1_000, 230_000);
    expect(mix).toMatchObject({ crossfades: false, mixPoint: null });
  });

  it("keeps a point a short track sets itself", () => {
    expect(mixOf(track, { crossfade: {} }, settings(), 0, 5_000)).toMatchObject({ shortTrack: true, mixPoint: null });
    expect(mixOf(track, { crossfade: { mode: "fixed", fixed_ms: 1_000 } }, settings(), 0, 5_000).mixPoint).toBe(4_000);
    expect(mixOf(track, { crossfade: { crossfade: false, mode: "fixed", fixed_ms: 1_000 } }, settings(), 0, 5_000).mixPoint).toBeNull();
  });

  it("takes the fades of the track before the station's", () => {
    const own = { enabled: false, time_ms: 50, curve: "linear", level_pct: 100 };
    const mix = mixOf(track, { crossfade: { fade_in: own, fade_out: own } }, settings(), 0, 100_000);
    expect(mix.fadeIn).toBe(own);
    expect(mix.fadeOut).toBe(own);
    expect(mixOf(track, { crossfade: {} }, undefined, 0, 100_000)).toMatchObject({ shortTrack: false, fadeIn: undefined });
  });
});

describe("annotations", () => {
  const fade = { enabled: true, time_ms: 3_000, curve: "linear" };
  const input = (changes: Record<string, unknown> = {}) => ({
    canEdit: true,
    cueIn: 1_000,
    cueOut: 230_000,
    length: 240_000,
    mixPoint: 220_000,
    shortTrack: false,
    crossfades: true,
    fadeOut: fade,
    fadeIn: fade,
    intro: 8_000,
    imported: { intro2_ms: 12_000 },
    hook: { in: 60_000, out: 80_000 },
    hookSet: true,
    setDraft: vi.fn(),
    setIntro: vi.fn(),
    setHook: vi.fn(),
    ...changes,
  });

  it("flags every point and draws the fades and the hook", () => {
    const { markers, regions } = annotations(input() as never);
    expect(markers.map((m) => m.id)).toEqual(["cue_in", "cue_out", "mix", "intro", "intro2", "hook_in", "hook_out"]);
    expect(regions.map((r) => r.label)).toEqual([t("track.fadeOut"), t("track.fadeIn"), t("track.hook")]);
  });

  it("flags the cue-out when the next track follows back to back", () => {
    const { markers } = annotations(input({ mixPoint: null }) as never);
    const mix = markers.find((m) => m.id === "mix");
    expect(mix?.ms).toBe(230_000);
    expect(mix?.label).toBe(t("transition.nextStartsBackToBack"));
  });

  it("draws only what is there, and nothing to drag for a viewer", () => {
    const { markers, regions } = annotations(
      input({ canEdit: false, intro: null, imported: null, hook: { in: null, out: null }, hookSet: false, mixPoint: null, fadeIn: undefined, fadeOut: undefined }) as never,
    );
    expect(markers.map((m) => m.id)).toEqual(["cue_in", "cue_out", "mix"]);
    expect(markers.every((m) => m.onDrag === undefined)).toBe(true);
    expect(regions).toEqual([]);
  });

  it("moves the points when they are dragged, within the file", () => {
    const given = input();
    const { markers } = annotations(given as never);
    const drag = (id: string, ms: number) => markers.find((m) => m.id === id)?.onDrag?.(ms, false);
    const applied = (mock: ReturnType<typeof vi.fn>, before: unknown) => (mock.mock.calls.at(-1)?.[0] as (x: unknown) => unknown)(before);

    drag("cue_in", 999_999);
    expect(applied(given.setDraft, { crossfade: {} })).toMatchObject({ cue_in_ms: 229_900 });
    drag("cue_out", 0);
    expect(applied(given.setDraft, { crossfade: {} })).toMatchObject({ cue_out_ms: 1_100 });
    drag("mix", 200_000);
    expect(applied(given.setDraft, { crossfade: {} })).toBeTruthy();
    drag("intro", 999_999);
    expect(given.setIntro).toHaveBeenLastCalledWith(240_000);
    drag("hook_in", 999_999);
    expect(applied(given.setHook, { in: 1, out: 70_000 })).toEqual({ in: 69_900, out: 70_000 });
    expect(applied(given.setHook, { in: 1, out: null })).toEqual({ in: 239_900, out: null });
    drag("hook_out", 0);
    expect(applied(given.setHook, { in: 50_000, out: 1 })).toEqual({ in: 50_000, out: 50_100 });
    expect(applied(given.setHook, { in: null, out: 1 })).toEqual({ in: null, out: 100 });
  });
});
