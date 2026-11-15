// @vitest-environment jsdom
// SPDX-License-Identifier: AGPL-3.0-or-later

import { screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { t } from "../i18n";
import { chooseStation } from "../lib/station";
import { answer, browserStubs, lenient, renderApp, user } from "../test/render";
import { Track } from "./Track";

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
  plays: { play_count: 3, last_played_at_unix_ms: null, stations: [] },
  programming: { mood: null, sound_code: null, era: null },
  likes: 0,
  dislikes: 0,
  play_ratio: 1,
  station_ratios: [],
  cover: null,
  cover_candidates: [],
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
  it("offers the actions to an operator, not to a viewer", async () => {
    page();
    expect(await screen.findByText(t("track.editTags"))).toBeTruthy();
  });

  it("hides the actions from a viewer", async () => {
    page(TRACK, "viewer");
    await screen.findByText("Blue Monday");
    expect(screen.queryByText(t("track.editTags"))).toBeNull();
  });

  it("asks to review a track that needs it", async () => {
    page({ ...TRACK, identification: { status: "needs_review", reason: "two releases" } });
    expect(await screen.findByText(t("track.reviewIt"))).toBeTruthy();
  });

});

