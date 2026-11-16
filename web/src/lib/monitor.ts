// SPDX-License-Identifier: AGPL-3.0-or-later

// The live monitor (docs/web.md#live-monitor): the station's sound from its
// engine, a fraction of a second behind the air, over a WebSocket
// (/api/v1/monitor). Opus is decoded with WebCodecs, PCM played as it
// comes; each 20 ms chunk is scheduled right after the last one in an
// AudioContext, with a short lead that absorbs the network's jitter.
// One player for the whole app: it keeps playing from page to page.

import { useSyncExternalStore } from "react";
import type { MonitorCodec, MonitorMessage, MonitorTap } from "../api";
import { stationUrl } from "./station";

export const SAMPLE_RATE = 48_000;
const CHANNELS = 2;
const CHUNK_FRAMES = 960;
const CHUNK_SECONDS = CHUNK_FRAMES / SAMPLE_RATE;
/** The lead a chunk is scheduled with after a gap: the jitter it absorbs. */
const LEAD_SECONDS = 0.08;
/** Beyond this much waiting to play, chunks are dropped to catch up. */
const MAX_AHEAD_SECONDS = 0.3;
const MAX_RETRY_MS = 5_000;
/** Connections closed before `ready` in a row before giving up. */
const MAX_REFUSALS = 3;

export type MonitorStatus =
  /** Not listening. */
  | "off"
  | "connecting"
  | "playing"
  /** The engine cannot be reached: the sound comes back with it. */
  | "unreachable"
  /** The engine has no live monitor (older than it). */
  | "too_old"
  /** The API refused or kept closing the connection. */
  | "failed";

export interface MonitorState {
  status: MonitorStatus;
  tap: MonitorTap;
  codec: MonitorCodec;
  /** 0 to 1. */
  volume: number;
  /** Seconds between a chunk arriving and being heard, `null` when not playing. */
  delay: number | null;
  /** Chunks lost since listening, the API's and the browser's. */
  lost: number;
}

/** Whether this browser decodes Opus (WebCodecs). */
export function opusSupported(): boolean {
  return typeof globalThis.AudioDecoder === "function" && typeof globalThis.EncodedAudioChunk === "function";
}

const KEY = "heron.monitor";

function stored(): Pick<MonitorState, "tap" | "codec" | "volume"> {
  const defaults = { tap: "processed" as MonitorTap, codec: "opus" as MonitorCodec, volume: 0.8 };
  try {
    const saved = JSON.parse(localStorage.getItem(KEY) ?? "{}") as Partial<MonitorState>;
    return {
      tap: saved.tap === "raw" ? "raw" : "processed",
      codec: saved.codec === "pcm" ? "pcm" : "opus",
      volume: typeof saved.volume === "number" ? Math.min(1, Math.max(0, saved.volume)) : defaults.volume,
    };
  } catch {
    return defaults;
  }
}

function save() {
  try {
    localStorage.setItem(KEY, JSON.stringify({ tap: state.tap, codec: state.codec, volume: state.volume }));
  } catch {
    // Not kept: the defaults next time.
  }
}

let state: MonitorState = { status: "off", delay: null, lost: 0, ...stored() };
const listeners = new Set<() => void>();

function set(changes: Partial<MonitorState>) {
  state = { ...state, ...changes };
  for (const listener of listeners) {
    listener();
  }
}

/**
 * Places chunks one after the other on the AudioContext's clock: right
 * after the last one, or after a short lead when the last one is already
 * played (a gap); `null` drops a chunk when too much is waiting.
 */
export class Scheduler {
  private next = 0;

  place(now: number, duration = CHUNK_SECONDS): number | null {
    if (this.next < now + 0.01) {
      this.next = now + LEAD_SECONDS;
    } else if (this.next > now + MAX_AHEAD_SECONDS) {
      return null;
    }
    const start = this.next;
    this.next += duration;
    return start;
  }

  /** How long until what is scheduled has played. */
  ahead(now: number): number {
    return Math.max(0, this.next - now);
  }
}

/** PCM chunk (16-bit little-endian, interleaved) as one array per channel. */
export function pcmChannels(data: ArrayBuffer): Float32Array[] {
  const view = new DataView(data);
  const frames = Math.floor(data.byteLength / (2 * CHANNELS));
  const left = new Float32Array(frames);
  const right = new Float32Array(frames);
  for (let frame = 0; frame < frames; frame++) {
    left[frame] = view.getInt16(frame * 4, true) / 32768;
    right[frame] = view.getInt16(frame * 4 + 2, true) / 32768;
  }
  return [left, right];
}

/** What a text message changes. */
export function onMessage(message: MonitorMessage, current: MonitorState): Partial<MonitorState> {
  switch (message.type) {
    case "ready":
      return { status: "playing", tap: message.tap, codec: message.codec };
    case "tap":
      return { tap: message.tap };
    case "lost":
      return { lost: current.lost + message.chunks };
    case "engine":
      return { status: message.reachable ? "playing" : "unreachable", delay: message.reachable ? current.delay : null };
    case "too_old":
      return { status: "too_old", delay: null };
  }
}

/** The monitor's address for the station shown. */
export function monitorUrl(location: Pick<Location, "protocol" | "host">, codec: MonitorCodec, tap: MonitorTap): string {
  const scheme = location.protocol === "https:" ? "wss" : "ws";
  return `${scheme}://${location.host}${stationUrl("monitor")}?codec=${codec}&tap=${tap}`;
}

// The player: one connection, one AudioContext, one decoder.
let socket: WebSocket | null = null;
let context: AudioContext | null = null;
let gain: GainNode | null = null;
let decoder: AudioDecoder | null = null;
let scheduler = new Scheduler();
let retry: ReturnType<typeof setTimeout> | null = null;
let retryDelay = 500;
let refusals = 0;
let packets = 0;
let wanted = false;

function play(channels: Float32Array[]) {
  if (!context || !gain) {
    return;
  }
  const frames = channels[0]?.length ?? 0;
  if (frames === 0) {
    return;
  }
  const start = scheduler.place(context.currentTime, frames / SAMPLE_RATE);
  if (start === null) {
    set({ lost: state.lost + 1 });
    return;
  }
  const buffer = context.createBuffer(CHANNELS, frames, SAMPLE_RATE);
  channels.forEach((samples, channel) => buffer.copyToChannel(samples as Float32Array<ArrayBuffer>, channel));
  const source = context.createBufferSource();
  source.buffer = buffer;
  source.connect(gain);
  source.start(start);
  const delay = scheduler.ahead(context.currentTime) + (context.outputLatency || context.baseLatency || 0);
  // Shown rounded to 10 ms: no need to re-render for less.
  if (state.delay === null || Math.abs(delay - state.delay) >= 0.01) {
    set({ delay });
  }
}

function decoded(data: AudioData) {
  const channels = Array.from({ length: data.numberOfChannels }, (_, plane) => {
    const samples = new Float32Array(data.numberOfFrames);
    data.copyTo(samples, { planeIndex: plane, format: "f32-planar" });
    return samples;
  });
  data.close();
  play(channels);
}

function openDecoder(): AudioDecoder {
  const opus = new AudioDecoder({
    output: decoded,
    error: () => {
      // A broken stream: start again from the next packet.
      if (decoder === opus) {
        decoder = null;
      }
    },
  });
  opus.configure({ codec: "opus", sampleRate: SAMPLE_RATE, numberOfChannels: CHANNELS });
  return opus;
}

function receive(data: ArrayBuffer) {
  if (state.codec === "pcm") {
    play(pcmChannels(data));
    return;
  }
  decoder ??= openDecoder();
  decoder.decode(
    new EncodedAudioChunk({ type: "key", timestamp: packets * CHUNK_SECONDS * 1e6, data }),
  );
  packets += 1;
}

function connect() {
  if (!wanted || socket) {
    return;
  }
  const codec = state.codec === "opus" && !opusSupported() ? "pcm" : state.codec;
  const ws = new WebSocket(monitorUrl(window.location, codec, state.tap));
  ws.binaryType = "arraybuffer";
  socket = ws;
  let ready = false;
  set({ status: state.status === "unreachable" ? "unreachable" : "connecting", codec });
  ws.onmessage = (event: MessageEvent<ArrayBuffer | string>) => {
    if (socket !== ws) {
      return;
    }
    if (typeof event.data === "string") {
      try {
        const message = JSON.parse(event.data) as MonitorMessage;
        if (message.type === "ready") {
          ready = true;
          refusals = 0;
          retryDelay = 500;
        }
        set(onMessage(message, state));
      } catch {
        // A message this version does not know: ignored.
      }
    } else {
      receive(event.data);
    }
  };
  ws.onclose = () => {
    if (socket !== ws) {
      return;
    }
    socket = null;
    if (!wanted || state.status === "too_old") {
      return;
    }
    refusals = ready ? 0 : refusals + 1;
    if (refusals >= MAX_REFUSALS) {
      wanted = false;
      set({ status: "failed", delay: null });
      return;
    }
    set({ status: "connecting", delay: null });
    retry = setTimeout(connect, retryDelay);
    retryDelay = Math.min(retryDelay * 2, MAX_RETRY_MS);
  };
}

function disconnect() {
  if (retry) {
    clearTimeout(retry);
    retry = null;
  }
  const closing = socket;
  socket = null;
  closing?.close();
  try {
    decoder?.close();
  } catch {
    // Already closed after an error.
  }
  decoder = null;
  packets = 0;
  scheduler = new Scheduler();
}

/** Starts listening to the station shown; from a click (browsers allow sound after one). */
export function startMonitor() {
  if (!context) {
    context = new AudioContext({ sampleRate: SAMPLE_RATE, latencyHint: "interactive" });
    gain = context.createGain();
    gain.connect(context.destination);
  }
  if (gain) {
    gain.gain.value = state.volume;
  }
  void context.resume();
  wanted = true;
  refusals = 0;
  retryDelay = 500;
  set({ status: "connecting", delay: null, lost: 0 });
  connect();
}

/** Stops listening (the button, signing out). */
export function stopMonitor() {
  wanted = false;
  disconnect();
  void context?.suspend();
  set({ status: "off", delay: null });
}

/** The station shown changed: listen to the new one, if listening. */
export function restartMonitor() {
  if (!wanted) {
    return;
  }
  disconnect();
  set({ status: "connecting", delay: null });
  connect();
}

/** Before or after the processing: switched on the next 20 ms, without a gap. */
export function setMonitorTap(tap: MonitorTap) {
  set({ tap });
  save();
  if (socket?.readyState === WebSocket.OPEN) {
    socket.send(JSON.stringify({ tap }));
  }
}

/** Opus or uncompressed: the connection is opened again with it. */
export function setMonitorCodec(codec: MonitorCodec) {
  if (codec === state.codec) {
    return;
  }
  set({ codec });
  save();
  restartMonitor();
}

export function setMonitorVolume(volume: number) {
  set({ volume });
  save();
  if (gain && context) {
    gain.gain.setTargetAtTime(volume, context.currentTime, 0.02);
  }
}

function subscribe(listener: () => void) {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

export function monitorState(): MonitorState {
  return state;
}

export function useMonitor<T>(select: (state: MonitorState) => T): T {
  return useSyncExternalStore(subscribe, () => select(state));
}
