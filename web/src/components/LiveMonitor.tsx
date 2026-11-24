// SPDX-License-Identifier: AGPL-3.0-or-later

// The live monitor's button and panel (docs/web.md#live-monitor): next to
// Heron's name at the top of the menu, and in the top bar on a phone.

import { useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { can } from "../api";
import { t } from "../i18n";
import {
  type MonitorState,
  opusSupported,
  setMonitorCodec,
  setMonitorTap,
  setMonitorVolume,
  startMonitor,
  stopMonitor,
  useMonitor,
} from "../lib/monitor";
import { useUser } from "../lib/session";
import { stationRole, useStation } from "../lib/station";
import { Icon } from "./ui";

const PANEL_WIDTH = 288;

/** The line under the switch: what is heard, or why not. */
export function monitorLine(state: Pick<MonitorState, "status" | "delay" | "lost">): string {
  switch (state.status) {
    case "off":
      return t("monitor.off");
    case "connecting":
      return t("monitor.connecting");
    case "playing": {
      const delay =
        state.delay === null
          ? t("monitor.playing")
          : t("monitor.delay", { ms: Math.round((state.delay * 1000) / 10) * 10 });
      return state.lost > 0 ? `${delay} · ${t("monitor.lost", { count: state.lost })}` : delay;
    }
    case "unreachable":
      return t("monitor.unreachable");
    case "too_old":
      return t("monitor.tooOld");
    case "failed":
      return t("monitor.failed");
  }
}

export function LiveMonitorButton() {
  const me = useUser();
  const station = useStation();
  const status = useMonitor((state) => state.status);
  const [open, setOpen] = useState<{ top: number; left: number } | null>(null);
  const button = useRef<HTMLButtonElement>(null);
  const panel = useRef<HTMLDialogElement>(null);

  useEffect(() => {
    if (!open) {
      return;
    }
    const outside = (event: MouseEvent) => {
      const target = event.target as Node;
      if (!panel.current?.contains(target) && !button.current?.contains(target)) {
        setOpen(null);
      }
    };
    const escape = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        setOpen(null);
        button.current?.focus();
      }
    };
    document.addEventListener("mousedown", outside);
    document.addEventListener("keydown", escape);
    return () => {
      document.removeEventListener("mousedown", outside);
      document.removeEventListener("keydown", escape);
    };
  }, [open]);

  if (!can(stationRole(me, station), "operator")) {
    return null;
  }
  const listening = status !== "off" && status !== "failed" && status !== "too_old";
  const toggle = () => {
    if (open) {
      setOpen(null);
      return;
    }
    // Fixed, placed under the button.
    const rect = button.current?.getBoundingClientRect();
    const top = (rect?.bottom ?? 0) + 4;
    const left = Math.max(8, Math.min((rect?.left ?? 0), window.innerWidth - PANEL_WIDTH - 8));
    setOpen({ top, left });
  };
  return (
    <>
      <button
        ref={button}
        type="button"
        className={listening ? "btn ghost icon monitor-button on" : "btn ghost icon monitor-button"}
        aria-label={t("monitor.button")}
        title={t("monitor.button")}
        aria-expanded={open !== null}
        onClick={toggle}
      >
        <Icon name="headphones" />
        {status === "playing" && (
          // Notes coming out of the headphones while the sound plays.
          <span className="monitor-notes" aria-hidden="true">
            <span>♪</span>
            <span>♫</span>
            <span>♪</span>
          </span>
        )}
      </button>
      {open &&
        // On the page's body: the menu is sticky, a stacking context of its
        // own, and would keep the panel under the main column.
        createPortal(
          <dialog
            open
            ref={panel}
            className="menu-panel monitor-panel stack"
            aria-label={t("monitor.title")}
            style={{ position: "fixed", top: open.top, left: open.left, width: PANEL_WIDTH }}
          >
            <MonitorPanel listening={listening} />
          </dialog>,
          document.body,
        )}
    </>
  );
}

function MonitorPanel({ listening }: Readonly<{ listening: boolean }>) {
  const state = useMonitor((current) => current);
  const opus = opusSupported();
  return (
    <>
      <div className="row" style={{ justifyContent: "space-between", alignItems: "center" }}>
        <strong>{t("monitor.title")}</strong>
        <button
          type="button"
          className={listening ? "btn small" : "btn primary small"}
          onClick={() => (listening ? stopMonitor() : startMonitor())}
        >
          <Icon name={listening ? "stop" : "play"} size={14} /> {listening ? t("monitor.stop") : t("monitor.listen")}
        </button>
      </div>
      <output className="hint" style={{ margin: 0 }}>
        {monitorLine(state)}
      </output>
      <fieldset className="segmented" aria-label={t("monitor.tap")}>
        {(["processed", "raw"] as const).map((tap) => (
          <button key={tap} type="button" aria-pressed={state.tap === tap} onClick={() => setMonitorTap(tap)}>
            {tap === "processed" ? t("monitor.processed") : t("monitor.raw")}
          </button>
        ))}
      </fieldset>
      <label className="field">
        <span>{t("monitor.quality")}</span>
        <select
          className="input"
          value={opus ? state.codec : "pcm"}
          onChange={(event) => setMonitorCodec(event.target.value === "pcm" ? "pcm" : "opus")}
        >
          <option value="opus" disabled={!opus}>
            {t("monitor.opus")}
          </option>
          <option value="pcm">{t("monitor.pcm")}</option>
        </select>
        <span className="hint">{opus ? t("monitor.qualityHint") : t("monitor.noOpus")}</span>
      </label>
      <label className="field">
        <span>{t("monitor.volume")}</span>
        <input
          type="range"
          min={0}
          max={1}
          step={0.05}
          value={state.volume}
          onChange={(event) => setMonitorVolume(Number(event.target.value))}
        />
      </label>
    </>
  );
}
