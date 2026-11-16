// @vitest-environment jsdom
// SPDX-License-Identifier: AGPL-3.0-or-later

import { act, fireEvent, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { stopMonitor } from "../lib/monitor";
import { chooseStation } from "../lib/station";
import { renderApp, user } from "../test/render";
import { LiveMonitorButton, monitorLine } from "./LiveMonitor";

class FakeSocket {
  static OPEN = 1;
  static last: FakeSocket;
  readyState = 1;
  binaryType = "blob";
  sent: string[] = [];
  onmessage: ((event: { data: string }) => void) | null = null;
  onclose: (() => void) | null = null;
  constructor(public url: string) {
    FakeSocket.last = this;
  }
  send(data: string) {
    this.sent.push(data);
  }
  close() {}
}

class FakeContext {
  currentTime = 0;
  destination = {};
  createGain() {
    return { connect() {}, gain: { value: 1, setTargetAtTime() {} } };
  }
  resume() {
    return Promise.resolve();
  }
  suspend() {
    return Promise.resolve();
  }
}

describe("LiveMonitorButton", () => {
  beforeEach(() => {
    vi.stubGlobal("WebSocket", FakeSocket);
    vi.stubGlobal("AudioContext", FakeContext);
    localStorage.clear();
    chooseStation(1);
  });
  afterEach(() => {
    stopMonitor();
    vi.unstubAllGlobals();
  });

  it("is not there for viewers", () => {
    renderApp(<LiveMonitorButton />, user("viewer"));
    expect(screen.queryByRole("button", { name: "Live monitor" })).toBeNull();
  });

  it("is there for operators of the station shown", () => {
    renderApp(<LiveMonitorButton />, { ...user("viewer"), station_roles: [{ station_id: 1, role: "operator" }] });
    expect(screen.getByRole("button", { name: "Live monitor" })).toBeTruthy();
  });

  it("listens, switches the tap and stops from its panel", () => {
    renderApp(<LiveMonitorButton />);
    fireEvent.click(screen.getByRole("button", { name: "Live monitor" }));
    const panel = screen.getByRole("dialog", { name: "Live monitor" });
    expect(panel.textContent).toContain("a fraction of a second behind the air");
    // Without WebCodecs, uncompressed only.
    expect(screen.getByText("This browser cannot decode Opus: uncompressed only.")).toBeTruthy();

    fireEvent.click(screen.getByRole("button", { name: /Listen/ }));
    expect(FakeSocket.last.url).toContain("/api/v1/stations/1/monitor?codec=pcm&tap=processed");
    act(() =>
      FakeSocket.last.onmessage?.({
        data: JSON.stringify({ type: "ready", codec: "pcm", sample_rate: 48000, channels: 2, frame_ms: 20, tap: "processed" }),
      }),
    );
    expect(screen.getByRole("status").textContent).toBe("Playing");
    expect(screen.getByRole("button", { name: "Live monitor" }).className).toContain("on");
    // Notes come out of the headphones while the sound plays.
    expect(screen.getByRole("button", { name: "Live monitor" }).querySelector(".monitor-notes")).not.toBeNull();

    fireEvent.click(screen.getByRole("button", { name: "Raw" }));
    expect(FakeSocket.last.sent).toEqual(['{"tap":"raw"}']);
    expect(screen.getByRole("button", { name: "Raw" }).getAttribute("aria-pressed")).toBe("true");

    fireEvent.click(screen.getByRole("button", { name: /Stop/ }));
    expect(screen.getByRole("button", { name: "Live monitor" }).querySelector(".monitor-notes")).toBeNull();
    expect(screen.getByRole("status").textContent).toContain("a fraction of a second");

    // Escape closes the panel.
    fireEvent.keyDown(document, { key: "Escape" });
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("says what is heard, or why not", () => {
    expect(monitorLine({ status: "playing", delay: 0.234, lost: 0 })).toBe("Playing, about 230 ms behind");
    expect(monitorLine({ status: "playing", delay: 0.1, lost: 2 })).toBe("Playing, about 100 ms behind · 2 chunks lost");
    expect(monitorLine({ status: "connecting", delay: null, lost: 0 })).toBe("Connecting…");
    expect(monitorLine({ status: "unreachable", delay: null, lost: 0 })).toContain("engine cannot be reached");
    expect(monitorLine({ status: "too_old", delay: null, lost: 0 })).toContain("too old");
    expect(monitorLine({ status: "failed", delay: null, lost: 0 })).toContain("could not be opened");
  });
});
