// SPDX-License-Identifier: AGPL-3.0-or-later

// The line at the bottom of every page (docs/web.md#status-bar): the decks
// (with the intro and the countdowns of the deck on air), the last track of
// a category type, the listeners, the next fixed time, the queue, the
// outputs, the machine's load and the station's clock, as each user chooses
// in Account › Status bar; with several stations, a button opening a bar
// per other station.

import { useQuery } from "@tanstack/react-query";
import { type CSSProperties, useEffect, useState } from "react";
import { api, unwrap } from "../api";
import { t } from "../i18n";
import { clockTime, duration, shortTime, trackName } from "../lib/format";
import { useListenersNow } from "../lib/listeners";
import { positionNow, useLive } from "../lib/live";
import { useModule } from "../lib/modules";
import { Link } from "../lib/router";
import { useUser } from "../lib/session";
import { here, useStation } from "../lib/station";
import {
  type Countdown,
  type DeckSummary,
  type OtherStation,
  type StatusBarSettings,
  countdownText,
  listenersText,
  nextFixedTime,
  otherStations,
  outputsOf,
  queueLength,
  recordOf,
  summarizeDeck,
  summarizeSystem,
  whenOf,
} from "../lib/statusbar";
import { useStations } from "./Stations";

/** Whether the other stations' bars are open, kept per browser. */
const OPEN_KEY = "heron.statusbar.stations";

function storedOpen(): boolean {
  try {
    return localStorage.getItem(OPEN_KEY) === "open";
  } catch {
    return false;
  }
}

function keepOpen(open: boolean) {
  try {
    localStorage.setItem(OPEN_KEY, open ? "open" : "closed");
  } catch {
    // Storage unavailable: the choice lasts until the page reloads.
  }
}

/** Another station's bar: the deck on air and the time left, or why it is in trouble. */
function StationRow({
  station,
  colors,
  onSwitch,
}: Readonly<{ station: OtherStation; colors: CSSProperties; onSwitch?: ((id: number) => void) | undefined }>) {
  const deck = station.onAir;
  const trouble = troubleText(station);
  const left = deck ? `−${duration(deck.left)}` : "";
  return (
    <fieldset
      className={`statusbar statusbar-row ${station.trouble ? "statusbar-row-trouble" : "statusbar-row-ok"}`}
      aria-label={station.code}
    >
      {onSwitch ? (
        <button
          type="button"
          className="statusbar-row-code"
          title={t("statusbar.switchTo", { code: station.code })}
          onClick={() => onSwitch(station.id)}
        >
          {station.code}
        </button>
      ) : (
        <span className="statusbar-row-code">{station.code}</span>
      )}
      {deck && (
        <span className={`statusbar-deck statusbar-on-air statusbar-deck-${deck.letter.toLowerCase()}`} style={colors}>
          <span className="visually-hidden">
            {t("statusbar.otherOnAir", { deck: deck.letter, track: deck.text, left })}
          </span>
          <span className="statusbar-deck-progress" aria-hidden="true" style={{ width: `${(deck.progress * 100).toFixed(2)}%` }} />
          <span className="statusbar-deck-letter" aria-hidden="true">
            {deck.letter}
          </span>
          <span className="statusbar-deck-symbol" aria-hidden="true">
            {deck.paused ? "⏸" : "▶"}
          </span>
          <span className="statusbar-deck-text" aria-hidden="true">
            {deck.text}
          </span>
          <span className="statusbar-deck-time" aria-hidden="true">
            {left}
          </span>
        </span>
      )}
      {trouble && <span className="statusbar-part statusbar-bad">{trouble}</span>}
    </fieldset>
  );
}

/** The time now, in the clock `positionNow` uses, moved on once a second. */
function useNow() {
  const [now, setNow] = useState(() => performance.now());
  useEffect(() => {
    const timer = setInterval(() => setNow(performance.now()), 1_000);
    return () => clearInterval(timer);
  }, []);
  return now;
}

/** The countdown's text and class: `intro −0:07`, urgent in its last seconds; `−0:09` before the next track. */
function countdownView(countdown: Countdown) {
  const time = countdownText(countdown);
  const urgency = countdown.blink ? "statusbar-countdown-blink" : "statusbar-countdown-alert";
  if (countdown.kind === "intro") {
    return {
      text: t("statusbar.intro", { time }),
      spoken: t("statusbar.introLeft", { count: countdown.seconds }),
      className: countdown.urgent ? urgency : "statusbar-countdown-intro",
    };
  }
  return {
    text: time,
    spoken: t("statusbar.nextIn", { count: countdown.seconds }),
    className: countdown.blink ? urgency : "statusbar-countdown-end",
  };
}

function Deck({ summary, colors }: Readonly<{ summary: DeckSummary; colors: CSSProperties }>) {
  const state = t(`statusbar.${summary.status}`);
  const badge = summary.badge && t(`statusbar.${summary.badge}`);
  const countdown = summary.countdown && countdownView(summary.countdown);
  const spoken = [
    t("statusbar.deck", { deck: summary.letter, state }),
    badge,
    summary.text && `${summary.text} ${summary.time}`,
    countdown?.spoken,
  ]
    .filter(Boolean)
    .join(", ");
  const percent = (fraction: number) => `${(fraction * 100).toFixed(2)}%`;
  return (
    <Link
      to="/"
      className={`statusbar-deck statusbar-deck-${summary.letter.toLowerCase()}${summary.onAir ? " statusbar-on-air" : ""}`}
      style={colors}
    >
      {/* What the symbols and the cut title say, for screen readers. */}
      <span className="visually-hidden">{spoken}</span>
      {/* Where the intro ends, how far the track has played and how much of the intro, behind the text. */}
      {summary.intro !== null && (
        <span className="statusbar-deck-intro" aria-hidden="true" style={{ width: percent(summary.intro) }} />
      )}
      {summary.progress !== null && (
        <span className="statusbar-deck-progress" aria-hidden="true" style={{ width: percent(summary.progress) }} />
      )}
      {summary.progress !== null && summary.intro !== null && summary.progress > 0 && (
        <span
          className="statusbar-deck-intro-progress"
          aria-hidden="true"
          style={{ width: percent(Math.min(summary.progress, summary.intro)) }}
        />
      )}
      <span className="statusbar-deck-letter" aria-hidden="true">
        {summary.letter}
      </span>
      <span className="statusbar-deck-symbol" aria-hidden="true">
        {summary.symbol}
      </span>
      {badge && (
        <span className={`statusbar-badge statusbar-badge-${summary.badge}`} aria-hidden="true">
          [{badge}]
        </span>
      )}
      {summary.text && (
        <span className="statusbar-deck-text" aria-hidden="true">
          {summary.text}
        </span>
      )}
      {countdown ? (
        <span className={`statusbar-deck-time ${countdown.className}`} aria-hidden="true">
          {countdown.text}
        </span>
      ) : (
        summary.time && (
          <span className="statusbar-deck-time statusbar-deck-elapsed" aria-hidden="true">
            {summary.time}
          </span>
        )
      )}
    </Link>
  );
}

/** The last track played to its end, of the category type chosen; it changes when the track on air does. */
function History({ typeId, onAir }: Readonly<{ typeId: number | null | undefined; onAir: number | null }>) {
  const station = useStation();
  const last = useQuery({
    queryKey: ["statusbar", "history", station, typeId ?? null, onAir],
    queryFn: () =>
      unwrap(
        api.GET("/api/v1/stations/{station_id}/history", {
          params: {
            path: here(),
            query: { finished: true, limit: 1, ...(typeId == null ? {} : { category_type_id: typeId }) },
          },
        }),
      ),
    staleTime: 30_000,
  });
  const play = last.data?.[0];
  const text = (play && [play.artist, play.title].filter(Boolean).join(" – ")) || "–";
  return (
    <Link to="/history" className="statusbar-part statusbar-link statusbar-history" title={text}>
      <span>{t("statusbar.previous")}</span>
      <span className="statusbar-history-text">{text}</span>
    </Link>
  );
}

/** The next show to open (or open now), top or track at an exact time. */
function NextFixed({ now }: Readonly<{ now: number }>) {
  const station = useStation();
  const tops = useModule("top30");
  const player = useLive((live) => live.player);
  const shows = useQuery({
    queryKey: ["statusbar", "shows", station],
    queryFn: () => unwrap(api.GET("/api/v1/stations/{station_id}/shows", { params: { path: here() } })),
    staleTime: 60_000,
    refetchInterval: 60_000,
  });
  const topList = useQuery({
    queryKey: ["statusbar", "tops", station],
    queryFn: () => unwrap(api.GET("/api/v1/stations/{station_id}/tops", { params: { path: here() } })),
    enabled: tops,
    staleTime: 60_000,
    refetchInterval: 60_000,
  });
  const next = nextFixedTime(
    shows.data ?? [],
    tops ? (topList.data ?? []) : [],
    player?.queue ?? [],
    now,
    (show) => show.settings.website.title.trim() || t(`nav.${show.module === "match" ? "match" : "dedications"}`),
    (entry) => trackName(entry.track),
  );
  if (next === null) {
    return null;
  }
  const time = shortTime(next.at);
  let text: string;
  if (next.open) {
    text = t("statusbar.until", { name: next.name, time });
  } else {
    const when = whenOf(next.at, now);
    if (when.kind === "in") text = t("statusbar.inMinutes", { name: next.name, count: when.minutes });
    else if (when.kind === "at") text = t("statusbar.at", { name: next.name, time });
    else text = t("statusbar.onDay", { name: next.name, day: when.day, time });
  }
  return (
    <Link to={next.link} className="statusbar-part statusbar-link statusbar-next">
      {text}
    </Link>
  );
}

/** A bar per other station, when there are some. */
function StationRows({
  stations,
  colors,
  onSwitch,
}: Readonly<{ stations: OtherStation[]; colors: CSSProperties; onSwitch: ((id: number) => void) | undefined }>) {
  if (stations.length === 0) return null;
  return (
    <div className="statusbar-rows">
      {stations.map((station) => (
        <StationRow key={station.id} station={station} colors={colors} onSwitch={onSwitch} />
      ))}
    </div>
  );
}

/** The server's CPU, memory and load. */
function SystemPart({ summary }: Readonly<{ summary: ReturnType<typeof summarizeSystem> }>) {
  return (
    <span className="statusbar-part statusbar-system">
      <span className={`statusbar-${summary.cpuLevel}`}>cpu {summary.cpu}</span>
      <span className={`statusbar-${summary.memLevel}`}>mem {summary.mem}</span>
      <span className={`statusbar-${summary.loadLevel}`}>load {summary.load}</span>
    </span>
  );
}

/** What a station in trouble has. */
function troubleText(station: OtherStation): string | null {
  switch (station.trouble) {
    case "engine":
      return t("statusbar.engine");
    case "nothing":
      return t("statusbar.nothingOnAir");
    case "outputs":
      return t("statusbar.outputsDownShort", { names: station.outputsDown.join(", ") });
    default:
      return null;
  }
}

/** The station's code, which opens a bar per other station. */
function StationsButton({
  code,
  open,
  inTrouble,
  onClick,
}: Readonly<{ code: string | undefined; open: boolean; inTrouble: number; onClick: () => void }>) {
  return (
    <button
      type="button"
      className={`statusbar-station${!open && inTrouble ? " statusbar-station-trouble" : ""}`}
      aria-expanded={open}
      aria-label={t(open ? "statusbar.closeStations" : "statusbar.openStations", { count: inTrouble })}
      onClick={onClick}
    >
      <span aria-hidden="true">
        {code ?? "–"} {open ? "▾" : "▴"}
      </span>
      {!open && inTrouble > 0 && (
        <span className="statusbar-bad" aria-hidden="true">
          {t("statusbar.down", { count: inTrouble })}
        </span>
      )}
    </button>
  );
}

function ListenersLink({
  total,
  record,
}: Readonly<{ total: number | null | undefined; record: ReturnType<typeof recordOf> }>) {
  return (
    <Link to="/statistics" className="statusbar-part statusbar-link">
      <span>
        {t("statusbar.listeners")} {listenersText(total)}
      </span>
      {record && (
        <span className="statusbar-record" title={t("statusbar.recordOn", { day: record.day })}>
          {t("statusbar.record")} {record.listeners}
        </span>
      )}
    </Link>
  );
}

function OutputsLink({ outputs }: Readonly<{ outputs: NonNullable<ReturnType<typeof outputsOf>> }>) {
  return (
    <Link
      to="/outputs"
      className={`statusbar-part statusbar-link ${outputs.down.length ? "statusbar-bad" : "statusbar-good"}`}
      title={outputs.down.length ? t("statusbar.outputsDown", { names: outputs.down.join(", ") }) : undefined}
    >
      {t("statusbar.outputs", { connected: outputs.connected, total: outputs.total })}
    </Link>
  );
}

/** The decks the bar shows: the one on air alone when the settings say so. */
function shownDecks(settings: StatusBarSettings, onAir: string | undefined): ("A" | "B")[] {
  if (!settings.on_air_only || onAir === undefined) {
    return ["A", "B"];
  }
  return [onAir === "b" ? "B" : "A"];
}

/** The colors chosen in the settings, as the bar's CSS variables. */
function barColors(settings: StatusBarSettings): CSSProperties {
  const colors: Record<string, string> = {};
  if (settings.track_color) {
    colors["--deck"] = settings.track_color;
  }
  if (settings.intro_color) {
    colors["--statusbar-intro"] = settings.intro_color;
  }
  return colors as CSSProperties;
}

/** A class name, with the preview's when it is the one in Account. */
function withPreview(className: string, preview: boolean): string {
  return preview ? `${className} statusbar-preview` : className;
}

/**
 * The status bar. `settings` and `preview`: the one in Account › Status
 * bar, which shows settings not saved yet and stays in its place.
 * `onSwitch`: what a click on another station's code does.
 */
export function StatusBar({
  settings: draft,
  preview = false,
  onSwitch,
}: Readonly<{ settings?: StatusBarSettings; preview?: boolean; onSwitch?: ((id: number) => void) | undefined }>) {
  const user = useUser();
  const settings = draft ?? user.status_bar;
  const module = useModule("status_bar");
  const shown = preview || (module && settings.show);
  const wallNow = useNow();
  const connected = useLive((live) => live.connected);
  const player = useLive((live) => live.player);
  const positions = useLive((live) => live.positions);
  const system = useLive((live) => live.system);
  const listeners = useListenersNow();
  const statistics = useModule("listener_statistics");
  const others = useLive((live) => live.stations);
  const stationId = useStation();
  const code = useStations().find((station) => station.id === stationId)?.code;
  const [open, setOpen] = useState(storedOpen);

  // The page takes its room back when there is no bar.
  useEffect(() => {
    if (preview) {
      return;
    }
    document.documentElement.dataset.statusbar = shown ? "on" : "off";
    return () => {
      delete document.documentElement.dataset.statusbar;
    };
  }, [preview, shown]);
  if (!shown) {
    return null;
  }

  const now = Date.now();
  const paused = player?.paused ?? false;
  const onAir = player?.decks.find((deck) => deck.on_air && deck.entry);
  const deckSummary = (letter: "A" | "B") => {
    const name = letter.toLowerCase();
    const deck = player?.decks.find((d) => d.deck === name);
    const position = positionNow(positions, name, paused, wallNow);
    return summarizeDeck(letter, deck, paused, position?.position_ms ?? null, settings);
  };
  const letters = shownDecks(settings, onAir?.deck);
  const colors = barColors(settings);
  const summary = system ? summarizeSystem(system) : null;
  const record = recordOf(listeners.data?.record);
  const outputs = player ? outputsOf(player.outputs) : null;
  const several = others !== null && others.stations.length > 0;
  const stations = several ? otherStations(others.stations, settings, wallNow - others.receivedAt) : null;
  const toggle = () => {
    setOpen(!open);
    // The preview's button is a try: the bar's choice stays.
    if (!preview) {
      keepOpen(!open);
    }
  };

  return (
    <div className={withPreview("statusbar-dock", preview)}>
      {open && stations && <StationRows stations={stations.shown} colors={colors} onSwitch={preview ? undefined : onSwitch} />}
    <div
      className={withPreview(connected ? "statusbar" : "statusbar statusbar-offline", preview)}
      role="status"
      aria-live="off"
      aria-label={t("statusbar.label")}
    >
      {stations && <StationsButton code={code} open={open} inTrouble={stations.inTrouble} onClick={toggle} />}
      {settings.decks &&
        (player === null ? (
          <span className="statusbar-deck">{t("statusbar.engine")}</span>
        ) : (
          letters.map((letter) => <Deck key={letter} summary={deckSummary(letter)} colors={colors} />)
        ))}
      {settings.history && <History typeId={settings.history_type_id} onAir={onAir?.entry?.entry_id ?? null} />}
      {settings.listeners && statistics && <ListenersLink total={listeners.data?.total} record={record} />}
      {settings.next_fixed && <NextFixed now={now} />}
      {settings.queue && player && (
        <Link to="/queue" className="statusbar-part statusbar-link statusbar-queue">
          {t("statusbar.queue", { time: duration(queueLength(player.queue)) })}
        </Link>
      )}
      {settings.outputs && outputs && outputs.total > 0 && <OutputsLink outputs={outputs} />}
      {settings.system && summary && <SystemPart summary={summary} />}
      {settings.station_time && <span className="statusbar-part statusbar-clock">{clockTime(now)}</span>}
      {!connected && <span className="statusbar-part">{t("statusbar.offline")}</span>}
    </div>
    </div>
  );
}
