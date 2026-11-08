// @vitest-environment jsdom
// SPDX-License-Identifier: AGPL-3.0-or-later

import { act, fireEvent, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { t } from "../i18n";
import { startLive, stopLive } from "../lib/live";
import { chooseStation } from "../lib/station";
import { answer, browserStubs, lenient, renderApp, sent, user } from "../test/render";
import { Dashboard } from "./Dashboard";

browserStubs();

/** The socket the page opens: the test speaks for the engine. */
class FakeSocket {
  static last: FakeSocket | null = null;
  onopen: (() => void) | null = null;
  onmessage: ((event: { data: string }) => void) | null = null;
  onclose: (() => void) | null = null;
  constructor() {
    FakeSocket.last = this;
  }
  close() {}
  say(message: unknown) {
    act(() => this.onmessage?.({ data: JSON.stringify(message) }));
  }
}

const track = (id: number, title: string, intro: number | null = 4_000) => ({
  entry_id: id,
  track: {
    track_id: id,
    title,
    artist: "New Order",
    album: "Power",
    path: `a/${id}.mp3`,
    cue_in_ms: 1_000,
    cue_out_ms: 200_000,
    duration_ms: 210_000,
    intro_ms: intro,
    no_metadata: false,
    over_intro: false,
    source: "rotation",
  },
});

function player(paused = false) {
  return {
    type: "state",
    player: {
      cued: track(3, "Cued song"),
      cued_jingle: null,
      pending_jingle: track(4, "Station jingle", null),
      decks: [
        { deck: "a", entry: track(1, "Blue Monday"), length_ms: 200_000, on_air: true, position_ms: 30_000 },
        { deck: "b", entry: null, length_ms: 0, on_air: false, position_ms: 0 },
        { deck: "fx", entry: null, length_ms: 0, on_air: false, position_ms: 0 },
      ],
      engine_started_at_unix_ms: 1_700_000_000_000,
      outputs: [],
      paused,
      queue: [],
      rendered_ms: 0,
      underruns: 0,
    },
  };
}

function open(paused = false) {
  vi.stubGlobal("WebSocket", FakeSocket);
  answer(
    lenient({
      "GET /api/v1/stations/{station_id}/engine": { config: { state: "in_sync" }, version: "0.3.0" },
      "POST /api/v1/stations/{station_id}/player/skip": undefined,
      "POST /api/v1/stations/{station_id}/player/pause": undefined,
      "POST /api/v1/stations/{station_id}/player/resume": undefined,
    }),
  );
  chooseStation(1);
  renderApp(<Dashboard />, user("operator"));
  startLive();
  FakeSocket.last?.say(player(paused));
}

afterEach(() => {
  stopLive();
  FakeSocket.last = null;
});

describe("Dashboard with a player", () => {
  it("shows the track on air, the idle deck and what is next", async () => {
    open();
    expect(await screen.findByText("Blue Monday")).toBeTruthy();
    expect(screen.getByText(t("app.onAirBadge"))).toBeTruthy();
    expect(screen.getByText("Cued song")).toBeTruthy();
    expect(screen.getAllByText(t("dashboard.next")).length).toBeGreaterThan(0);
  });

  it("skips the track on air", async () => {
    open();
    await screen.findByText("Blue Monday");
    fireEvent.click(await screen.findByText(t("dashboard.skip"), { exact: false }));
    await waitFor(() => expect(sent("POST /api/v1/stations/{station_id}/player/skip")).toHaveLength(1));
  });

  it("pauses the player, and resumes it", async () => {
    open();
    await screen.findByText("Blue Monday");
    fireEvent.click(await screen.findByText(t("dashboard.pause")));
    fireEvent.click((await screen.findAllByText(t("dashboard.pause"))).at(-1) as HTMLElement);
    await waitFor(() => expect(sent("POST /api/v1/stations/{station_id}/player/pause")).toHaveLength(1));
  });

  it("offers to resume a paused player", async () => {
    open(true);
    await screen.findByText("Blue Monday");
    fireEvent.click(await screen.findByText(t("dashboard.resume")));
    await waitFor(() => expect(sent("POST /api/v1/stations/{station_id}/player/resume")).toHaveLength(1));
  });
});
