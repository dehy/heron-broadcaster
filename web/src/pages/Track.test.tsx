// @vitest-environment jsdom
// SPDX-License-Identifier: AGPL-3.0-or-later

import { fireEvent, screen, waitFor } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { t } from "../i18n";
import { chooseStation } from "../lib/station";
import { answer, browserStubs, lenient, renderApp, sent, user } from "../test/render";
import { Track, effectiveCues } from "./Track";

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
    await waitFor(() => expect(sent("POST /api/v1/stations/{station_id}/queue").length).toBe(1));
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
