// SPDX-License-Identifier: AGPL-3.0-or-later

// The line at the bottom of every page: both decks, the station's
// listeners and, for operators and admins, the machine's load
// (docs/web.md#status-bar).

import { useEffect, useState } from "react";
import { t } from "../i18n";
import { useListenersNow } from "../lib/listeners";
import { positionNow, useLive } from "../lib/live";
import { type DeckSummary, listenersText, summarizeDeck, summarizeSystem } from "../lib/statusbar";

/** Elapsed times move on once a second. */
function useSecond() {
  const [, tick] = useState(0);
  useEffect(() => {
    const timer = setInterval(() => tick((n) => n + 1), 1_000);
    return () => clearInterval(timer);
  }, []);
}

function Deck({ summary }: { summary: DeckSummary }) {
  const state = t(`statusbar.${summary.status}`);
  return (
    <span
      className={`statusbar-deck statusbar-deck-${summary.letter.toLowerCase()}${summary.onAir ? " statusbar-on-air" : ""}`}
      role="group"
      aria-label={`${t("statusbar.deck", { deck: summary.letter, state })}${summary.text ? `, ${summary.text}` : ""}`}
    >
      <span className="statusbar-deck-letter" aria-hidden="true">
        {summary.letter}
      </span>
      <span className="statusbar-deck-symbol" aria-hidden="true">
        {summary.symbol}
      </span>
      {summary.text && (
        <span className="statusbar-deck-text" aria-hidden="true">
          {summary.text}
        </span>
      )}
    </span>
  );
}

export function StatusBar() {
  useSecond();
  const connected = useLive((live) => live.connected);
  const player = useLive((live) => live.player);
  const positions = useLive((live) => live.positions);
  const system = useLive((live) => live.system);
  const listeners = useListenersNow();

  const paused = player?.paused ?? false;
  const deckSummary = (letter: "A" | "B") => {
    const name = letter.toLowerCase();
    const deck = player?.decks.find((d) => d.deck === name);
    const position = positionNow(positions, name, paused);
    return summarizeDeck(letter, deck, paused, position?.position_ms ?? null);
  };
  const summary = system ? summarizeSystem(system) : null;

  return (
    <div className={`statusbar${connected ? "" : " statusbar-offline"}`} role="status" aria-live="off" aria-label={t("statusbar.label")}>
      {player === null ? (
        <span className="statusbar-deck">{t("statusbar.engine")}</span>
      ) : (
        <>
          <Deck summary={deckSummary("A")} />
          <Deck summary={deckSummary("B")} />
        </>
      )}
      <span className="statusbar-part">
        {t("statusbar.listeners")} {listenersText(listeners.data?.total)}
      </span>
      {summary && (
        <span className="statusbar-part statusbar-system">
          <span className={`statusbar-${summary.cpuLevel}`}>cpu {summary.cpu}</span>
          <span className={`statusbar-${summary.memLevel}`}>mem {summary.mem}</span>
          <span className={`statusbar-${summary.loadLevel}`}>load {summary.load}</span>
        </span>
      )}
      {!connected && <span className="statusbar-part">{t("statusbar.offline")}</span>}
    </div>
  );
}
