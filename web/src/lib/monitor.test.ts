// @vitest-environment jsdom
// SPDX-License-Identifier: AGPL-3.0-or-later

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  Scheduler,
  monitorState,
  monitorUrl,
  onMessage,
  opusSupported,
  pcmChannels,
  restartMonitor,
  setMonitorCodec,
  setMonitorTap,
  setMonitorVolume,
  startMonitor,
  stopMonitor,
} from "./monitor";
import { chooseStation } from "./station";

/** A WebSocket the test drives. */
class FakeSocket {
  static OPEN = 1;
  static all: FakeSocket[] = [];
  readyState = 1;
  binaryType = "blob";
  sent: string[] = [];
  onmessage: ((event: { data: ArrayBuffer | string }) => void) | null = null;
  onclose: (() => void) | null = null;
  constructor(public url: string) {
    FakeSocket.all.push(this);
  }
  static get last(): FakeSocket {
    const last = FakeSocket.all.at(-1);
    if (!last) {
      throw new Error("no socket");
    }
    return last;
  }
  send(data: string) {
    this.sent.push(data);
  }
  close() {
    this.readyState = 3;
  }
  text(message: unknown) {
    this.onmessage?.({ data: JSON.stringify(message) });
  }
}

/** An AudioContext that records what is scheduled. */
class FakeContext {
  static last: FakeContext;
  currentTime = 10;
  baseLatency = 0.01;
  outputLatency = 0;
  destination = {};
  started: number[] = [];
  gainValue = 0;
  constructor() {
    FakeContext.last = this;
  }
  createGain() {
    const ctx = this;
    return {
      connect() {},
      gain: {
        set value(v: number) {
          ctx.gainValue = v;
        },
        get value() {
          return ctx.gainValue;
        },
        setTargetAtTime(v: number) {
          ctx.gainValue = v;
        },
      },
    };
  }
  createBuffer(_channels: number, frames: number) {
    return { frames, copyToChannel() {} };
  }
  createBufferSource() {
    const started = this.started;
    return {
      buffer: null,
      connect() {},
      start(at: number) {
        started.push(at);
      },
    };
  }
  resume() {
    return Promise.resolve();
  }
  suspend() {
    return Promise.resolve();
  }
}

const READY = { type: "ready", codec: "pcm", sample_rate: 48000, channels: 2, frame_ms: 20, tap: "processed" };

function pcm(left: number, right: number): ArrayBuffer {
  const view = new DataView(new ArrayBuffer(960 * 4));
  for (let frame = 0; frame < 960; frame++) {
    view.setInt16(frame * 4, left, true);
    view.setInt16(frame * 4 + 2, right, true);
  }
  return view.buffer;
}

describe("Scheduler", () => {
  it("plays chunks back to back after a short lead", () => {
    const scheduler = new Scheduler();
    expect(scheduler.place(10)).toBeCloseTo(10.08);
    expect(scheduler.place(10.005)).toBeCloseTo(10.1);
    expect(scheduler.place(10.02)).toBeCloseTo(10.12);
    expect(scheduler.ahead(10.02)).toBeCloseTo(0.12);
  });

  it("starts again with the lead after a gap", () => {
    const scheduler = new Scheduler();
    scheduler.place(10);
    expect(scheduler.place(11)).toBeCloseTo(11.08);
  });

  it("drops chunks when too much waits, to catch up", () => {
    const scheduler = new Scheduler();
    const starts = Array.from({ length: 20 }, () => scheduler.place(10));
    expect(starts.filter((start) => start === null).length).toBeGreaterThan(0);
    expect(scheduler.ahead(10)).toBeLessThan(0.33);
  });
});

describe("the live monitor's messages", () => {
  const base = monitorState();

  it("splits PCM into channels", () => {
    const [left, right] = pcmChannels(pcm(16384, -32768));
    expect(left).toHaveLength(960);
    expect(left?.[0]).toBeCloseTo(0.5);
    expect(right?.[959]).toBe(-1);
  });

  it("follows the text messages", () => {
    expect(onMessage({ type: "ready", codec: "opus", sample_rate: 48000, channels: 2, frame_ms: 20, bitrate: 256000, tap: "raw" }, base)).toEqual({
      status: "playing",
      tap: "raw",
      codec: "opus",
    });
    expect(onMessage({ type: "tap", tap: "processed" }, base)).toEqual({ tap: "processed" });
    expect(onMessage({ type: "lost", chunks: 3 }, { ...base, lost: 2 })).toEqual({ lost: 5 });
    expect(onMessage({ type: "engine", reachable: false }, base)).toEqual({ status: "unreachable", delay: null });
    expect(onMessage({ type: "engine", reachable: true }, { ...base, delay: 0.1 })).toEqual({ status: "playing", delay: 0.1 });
    expect(onMessage({ type: "too_old" }, base)).toEqual({ status: "too_old", delay: null });
  });

  it("asks the station shown", () => {
    chooseStation(3);
    expect(monitorUrl({ protocol: "https:", host: "radio.example" }, "opus", "raw")).toBe(
      "wss://radio.example/api/v1/stations/3/monitor?codec=opus&tap=raw",
    );
    chooseStation(null);
    expect(monitorUrl({ protocol: "http:", host: "localhost" }, "pcm", "processed")).toBe(
      "ws://localhost/api/v1/monitor?codec=pcm&tap=processed",
    );
  });

  it("knows whether Opus can be decoded", () => {
    expect(opusSupported()).toBe(false);
  });
});

describe("the live monitor's player", () => {
  beforeEach(() => {
    FakeSocket.all = [];
    vi.stubGlobal("WebSocket", FakeSocket);
    vi.stubGlobal("AudioContext", FakeContext);
    vi.useFakeTimers();
    localStorage.clear();
    chooseStation(1);
  });
  afterEach(() => {
    stopMonitor();
    vi.useRealTimers();
    vi.unstubAllGlobals();
  });

  it("plays PCM without WebCodecs, and switches taps on the socket", () => {
    startMonitor();
    const socket = FakeSocket.last;
    // Opus is asked for, but this browser cannot decode it.
    expect(socket.url).toContain("codec=pcm");
    expect(socket.binaryType).toBe("arraybuffer");
    expect(monitorState().status).toBe("connecting");
    socket.text(READY);
    expect(monitorState().status).toBe("playing");
    socket.onmessage?.({ data: pcm(1000, 1000) });
    socket.onmessage?.({ data: pcm(1000, 1000) });
    expect(FakeContext.last.started).toHaveLength(2);
    expect(FakeContext.last.started[1]! - FakeContext.last.started[0]!).toBeCloseTo(0.02);
    expect(monitorState().delay).toBeCloseTo(0.13);

    setMonitorTap("raw");
    expect(socket.sent).toEqual(['{"tap":"raw"}']);
    socket.text({ type: "tap", tap: "raw" });
    expect(monitorState().tap).toBe("raw");
    setMonitorVolume(0.4);
    expect(FakeContext.last.gainValue).toBe(0.4);
    // Kept for next time.
    expect(JSON.parse(localStorage.getItem("heron.monitor") ?? "{}")).toEqual({ tap: "raw", codec: "pcm", volume: 0.4 });

    stopMonitor();
    expect(monitorState().status).toBe("off");
    expect(socket.readyState).toBe(3);
  });

  it("connects again after a drop, and gives up when refused", () => {
    startMonitor();
    FakeSocket.last.text(READY);
    FakeSocket.last.onclose?.();
    expect(monitorState().status).toBe("connecting");
    vi.advanceTimersByTime(500);
    expect(FakeSocket.all).toHaveLength(2);
    // Refused before `ready`, three times in a row.
    FakeSocket.last.onclose?.();
    vi.advanceTimersByTime(1_000);
    FakeSocket.last.onclose?.();
    vi.advanceTimersByTime(2_000);
    FakeSocket.last.onclose?.();
    expect(monitorState().status).toBe("failed");
    vi.advanceTimersByTime(10_000);
    expect(FakeSocket.all).toHaveLength(4);
  });

  it("stops for an engine too old, and starts again on another station", () => {
    startMonitor();
    FakeSocket.last.text({ type: "too_old" });
    FakeSocket.last.onclose?.();
    vi.advanceTimersByTime(10_000);
    expect(monitorState().status).toBe("too_old");
    expect(FakeSocket.all).toHaveLength(1);

    startMonitor();
    chooseStation(2);
    restartMonitor();
    expect(FakeSocket.last.url).toContain("/stations/2/monitor");
    setMonitorCodec("opus");
    expect(FakeSocket.all).toHaveLength(4);
  });
});
