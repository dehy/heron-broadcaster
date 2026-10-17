// SPDX-License-Identifier: AGPL-3.0-or-later

// On air: the decks, levels, transport, FX player, what comes next and the
// outputs.

import { useQuery } from "@tanstack/react-query";
import { useEffect, useState } from "react";
import { type DeckState, type OutputStatus, type QueueEntry, api, done, unwrap } from "../api";
import { StereoMeter, LineMeter } from "../components/Meter";
import { QueueList } from "../components/QueueList";
import { TrackSearch } from "../components/TrackSearch";
import { Card, ConfirmButton, Empty, Icon, Modal, PageHead } from "../components/ui";
import { ago, bytes, clockTime, duration, trackName } from "../lib/format";
import { positionNow, useLive } from "../lib/live";
import { Link } from "../lib/router";
import { useAction, useCan } from "../lib/session";

/** Re-renders `fps` times a second while mounted. */
export function useTicker(fps = 10): number {
  const [now, setNow] = useState(() => performance.now());
  useEffect(() => {
    let frame = 0;
    let last = 0;
    const step = (time: number) => {
      if (time - last >= 1000 / fps) {
        last = time;
        setNow(time);
      }
      frame = requestAnimationFrame(step);
    };
    frame = requestAnimationFrame(step);
    return () => cancelAnimationFrame(frame);
  }, [fps]);
  return now;
}

const DECK_NAMES: Record<string, string> = { a: "A", b: "B", fx: "FX" };
const DECK_COLORS: Record<string, string> = {
  a: "var(--deck-a)",
  b: "var(--deck-b)",
  fx: "var(--fx)",
};

export function Dashboard() {
  const player = useLive((live) => live.player);
  const levels = useLive((live) => live.levels);
  const canOperate = useCan("operator");
  const [fxOpen, setFxOpen] = useState(false);

  const skip = useAction(() => done(api.POST("/api/v1/player/skip")), { success: "Skipped" });
  const pause = useAction(() => done(api.POST("/api/v1/player/pause")));
  const resume = useAction(() => done(api.POST("/api/v1/player/resume")));
  const stopFx = useAction(() => done(api.DELETE("/api/v1/player/fx")));
  const playFx = useAction(
    (trackId: number) => unwrap(api.POST("/api/v1/player/fx", { body: { track_id: trackId } })),
    { success: "Playing on the FX player", onSuccess: () => setFxOpen(false) },
  );

  const decks = player?.decks.filter((deck) => deck.deck !== "fx") ?? [];
  const fx = player?.decks.find((deck) => deck.deck === "fx");
  const fxPlaying = Boolean(fx?.entry);

  return (
    <>
      <PageHead
        title="On air"
        actions={
          canOperate &&
          player && (
            <>
              {player.paused ? (
                <button
                  type="button"
                  className="btn primary big"
                  onClick={() => resume.mutate(undefined)}
                >
                  <Icon name="play" /> Resume
                </button>
              ) : (
                <ConfirmButton
                  className="btn big"
                  confirm="Pause"
                  question="Pausing puts silence on the air until you resume."
                  onConfirm={() => pause.mutate(undefined)}
                >
                  <Icon name="pause" /> Pause
                </ConfirmButton>
              )}
              <button
                type="button"
                className="btn primary big"
                disabled={skip.isPending || !decks.some((deck) => deck.on_air)}
                onClick={() => skip.mutate(undefined)}
              >
                <Icon name="skip" /> Skip
              </button>
            </>
          )
        }
      />

      {player?.paused && (
        <div className="banner warn" role="status">
          <Icon name="pause" /> The player is paused: the station is silent.
        </div>
      )}

      <div className="grid sidebar-right">
        <div className="stack">
          <div className="decks">
            {player === undefined && <Card><Empty>Connecting…</Empty></Card>}
            {decks.map((deck) => (
              <Deck
                key={deck.deck}
                deck={deck}
                paused={player?.paused ?? false}
                cued={!deck.entry && !deck.on_air ? (player?.cued ?? null) : null}
              />
            ))}
          </div>

          <Card
            title="Up next"
            actions={<Link to="/queue" className="btn small">Edit the queue</Link>}
            flush
          >
            <QueueList limit={6} />
          </Card>
        </div>

        <div className="stack">
          <Card title="Levels">
            <div className="row" style={{ alignItems: "flex-start", gap: "1.5rem" }}>
              <StereoMeter levels={levels?.master ?? null} label="Master" />
              <div className="stack" style={{ flex: 1, minWidth: "8rem" }}>
                {(["deck_a", "deck_b", "fx"] as const).map((key) => (
                  <div key={key} className="stack" style={{ gap: "0.2rem" }}>
                    <span className="small muted">
                      {key === "fx" ? "FX player" : `Deck ${key.slice(-1).toUpperCase()}`}
                    </span>
                    <LineMeter levels={levels?.[key] ?? null} label={key} />
                  </div>
                ))}
              </div>
            </div>
          </Card>

          <Card
            title="FX player"
            actions={
              canOperate && (
                <>
                  {fxPlaying && (
                    <button type="button" className="btn small" onClick={() => stopFx.mutate(undefined)}>
                      <Icon name="stop" size={14} /> Stop
                    </button>
                  )}
                  <button type="button" className="btn small" onClick={() => setFxOpen(true)}>
                    <Icon name="bolt" size={14} /> Play over
                  </button>
                </>
              )
            }
          >
            {fx?.entry ? (
              <FxState deck={fx} />
            ) : (
              <p className="muted small" style={{ margin: 0 }}>
                Jingles and sweepers played over the music, without stopping it.
              </p>
            )}
          </Card>

          <OutputsCard outputs={player?.outputs ?? []} />
          <EngineCard />
        </div>
      </div>

      <Modal title="Play on the FX player" open={fxOpen} onClose={() => setFxOpen(false)}>
        <TrackSearch
          placeholder="Search jingles, IDs, sweepers…"
          actions={(track) => (
            <button
              type="button"
              className="btn small primary"
              disabled={playFx.isPending}
              onClick={() => playFx.mutate(track.id)}
            >
              <Icon name="play" size={14} /> Play
            </button>
          )}
        />
      </Modal>
    </>
  );
}

function Deck({
  deck,
  paused,
  cued,
}: {
  deck: DeckState;
  paused: boolean;
  /** The next track, shown on the idle deck. */
  cued: QueueEntry | null;
}) {
  const positions = useLive((live) => live.positions);
  const now = useTicker(deck.on_air ? 10 : 2);
  const live = positionNow(positions, deck.deck, paused, now);
  const position = live?.position_ms ?? deck.position_ms;
  const track = deck.entry?.track ?? cued?.track;
  const cueOut = track?.cue_out_ms ?? deck.length_ms;
  const remaining = Math.max(0, cueOut - position);
  const progress = cueOut > 0 ? Math.min(1, position / cueOut) : 0;

  return (
    <section
      className={deck.on_air ? "card deck on-air" : "card deck"}
      style={{ ["--deck-color" as string]: DECK_COLORS[deck.deck] }}
      aria-label={`Deck ${DECK_NAMES[deck.deck]}`}
    >
      <div className="card-body stack">
        <div className="row between" style={{ flexWrap: "nowrap" }}>
          <div className="row" style={{ flexWrap: "nowrap", minWidth: 0 }}>
            <span className="deck-label">{DECK_NAMES[deck.deck]}</span>
            {deck.on_air ? (
              <span className="on-air-pill">ON AIR</span>
            ) : track ? (
              <span className="badge accent">Next</span>
            ) : (
              <span className="badge">Empty</span>
            )}
          </div>
          {track && (
            <span className="badge">{track.source === "rotation" ? "Autofill" : track.source}</span>
          )}
        </div>
        {track ? (
          <>
            <div style={{ minWidth: 0 }}>
              <div className="deck-title ellipsis">
                {track.track_id ? (
                  <Link to={`/library/${track.track_id}`}>{track.title ?? trackName(track)}</Link>
                ) : (
                  (track.title ?? trackName(track))
                )}
              </div>
              <div className="muted ellipsis">
                {[track.artist, track.album].filter(Boolean).join(" · ") || track.path}
              </div>
            </div>
            {cued ? (
              <div className="deck-times">
                <span className="muted">Starts at its cue-in</span>
                <span className="remaining">{duration(cueOut - (track.cue_in_ms ?? 0))}</span>
              </div>
            ) : (
              <>
                <div
                  className="progress"
                  role="progressbar"
                  aria-label="Position"
                  aria-valuemin={0}
                  aria-valuemax={Math.round(cueOut / 1000)}
                  aria-valuenow={Math.round(position / 1000)}
                >
                  <div style={{ width: `${progress * 100}%` }} />
                </div>
                <div className="deck-times">
                  <span className="muted">{duration(position)}</span>
                  <span className="remaining">-{duration(remaining)}</span>
                </div>
              </>
            )}
          </>
        ) : (
          <p className="muted" style={{ margin: 0 }}>
            Nothing loaded.
          </p>
        )}
      </div>
    </section>
  );
}

function FxState({ deck }: { deck: DeckState }) {
  const positions = useLive((live) => live.positions);
  const now = useTicker(5);
  const live = positionNow(positions, "fx", false, now);
  const track = deck.entry?.track;
  if (!track) {
    return null;
  }
  const remaining = Math.max(0, track.cue_out_ms - (live?.position_ms ?? deck.position_ms));
  return (
    <div className="row between">
      <span className="ellipsis">
        <span className="dot live" /> {trackName(track)}
      </span>
      <span className="num">-{duration(remaining)}</span>
    </div>
  );
}

const OUTPUT_STATES: Record<string, { label: string; dot: string }> = {
  connected: { label: "Connected", dot: "good" },
  connecting: { label: "Connecting", dot: "warn" },
  reconnecting: { label: "Reconnecting", dot: "bad" },
  stopped: { label: "Stopped", dot: "" },
};

export function OutputState({ status }: { status: OutputStatus | null | undefined }) {
  if (!status) {
    return <span className="badge">Not running</span>;
  }
  const state = OUTPUT_STATES[status.state] ?? { label: status.state, dot: "" };
  return (
    <span className="row small" style={{ gap: "0.35rem", flexWrap: "nowrap" }}>
      <span className={`dot ${state.dot}`} /> {status.enabled ? state.label : "Disabled"}
    </span>
  );
}

function OutputsCard({ outputs }: { outputs: OutputStatus[] }) {
  return (
    <Card
      title="Outputs"
      actions={<Link to="/outputs" className="btn small">Configure</Link>}
      flush
    >
      {outputs.length === 0 ? (
        <Empty>
          No output: nobody hears the station. <Link to="/outputs">Add one</Link>
        </Empty>
      ) : (
        <ul className="list">
          {outputs.map((output) => (
            <li key={output.id} className="queue-item" style={{ alignItems: "flex-start" }}>
              <div className="meta">
                <div className="row between" style={{ flexWrap: "nowrap" }}>
                  <span className="ellipsis">{output.name}</span>
                  <OutputState status={output} />
                </div>
                <div className="muted small">
                  {output.state === "connected" && output.connected_since_unix_ms
                    ? `Since ${clockTime(output.connected_since_unix_ms)} · ${bytes(output.bytes_sent)} sent`
                    : output.error
                      ? <span className="bad">{output.error}</span>
                      : null}
                  {output.dropped_chunks > 0 && (
                    <span className="warn"> · {output.dropped_chunks} chunks dropped</span>
                  )}
                </div>
              </div>
            </li>
          ))}
        </ul>
      )}
    </Card>
  );
}

function EngineCard() {
  const player = useLive((live) => live.player);
  const engine = useQuery({
    queryKey: ["engine"],
    queryFn: () => unwrap(api.GET("/api/v1/engine")),
    refetchInterval: 15_000,
  });
  const sync = engine.data?.config.state;
  return (
    <Card title="Engine">
      <dl className="facts small">
        <dt>Running since</dt>
        <dd>{player ? ago(player.engine_started_at_unix_ms) : "–"}</dd>
        <dt>Configuration</dt>
        <dd className={sync === "failed" ? "bad" : sync === "synced" ? "good" : "muted"}>
          {sync === "synced"
            ? "In sync"
            : sync === "pending"
              ? "Waiting for the engine"
              : sync === "failed"
                ? "Refused"
                : "Unknown"}
        </dd>
        <dt>Underruns</dt>
        <dd className={player && player.underruns > 0 ? "warn num" : "num"}>
          {player?.underruns ?? "–"}
        </dd>
      </dl>
    </Card>
  );
}
