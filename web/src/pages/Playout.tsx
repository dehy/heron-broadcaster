// SPDX-License-Identifier: AGPL-3.0-or-later

// Crossfade and loudness settings, with a live preview of a transition.

import { keepPreviousData, useQuery } from "@tanstack/react-query";
import { useEffect, useState } from "react";
import { type PlayoutSettings, type TrackSummary, api, unwrap } from "../api";
import { FadeEditor } from "../components/FadeEditor";
import { TrackSearch, useDebounced } from "../components/TrackSearch";
import { TransitionGraph } from "../components/TransitionGraph";
import { useWaveform } from "../components/Waveform";
import {
  Card,
  Empty,
  ErrorView,
  Field,
  Loading,
  Modal,
  NumberInput,
  PageHead,
  Segmented,
} from "../components/ui";
import { db, trackName } from "../lib/format";
import { useLive } from "../lib/live";
import { useAction, useCan } from "../lib/session";

export function Playout() {
  const settings = useQuery({
    queryKey: ["settings", "playout"],
    queryFn: () => unwrap(api.GET("/api/v1/settings/playout")),
  });
  if (settings.isPending) {
    return <Loading />;
  }
  if (settings.isError) {
    return <ErrorView error={settings.error} />;
  }
  return <PlayoutForm saved={settings.data} />;
}

interface Pick {
  id: number;
  name: string;
}

function PlayoutForm({ saved }: { saved: PlayoutSettings }) {
  const isAdmin = useCan("admin");
  const [draft, setDraft] = useState(saved);
  useEffect(() => setDraft(saved), [saved]);
  const dirty = JSON.stringify(draft) !== JSON.stringify(saved);
  const save = useAction(
    (settings: PlayoutSettings) => unwrap(api.PUT("/api/v1/settings/playout", { body: settings })),
    { success: "Saved: the engine uses them from the next transition", invalidate: [["settings", "playout"]] },
  );
  const crossfade = draft.crossfade;
  const loudness = draft.loudness;
  const setCrossfade = (changes: Partial<PlayoutSettings["crossfade"]>) =>
    setDraft((d) => ({ ...d, crossfade: { ...d.crossfade, ...changes } }));
  const setLoudness = (changes: Partial<PlayoutSettings["loudness"]>) =>
    setDraft((d) => ({ ...d, loudness: { ...d.loudness, ...changes } }));
  const disabled = !isAdmin;

  return (
    <>
      <PageHead
        title="Crossfade & loudness"
        description="How tracks hand over to each other, and how loud they play."
        actions={
          isAdmin && (
            <>
              {dirty && (
                <button type="button" className="btn" onClick={() => setDraft(saved)}>
                  Revert
                </button>
              )}
              <button
                type="button"
                className="btn primary"
                disabled={!dirty || save.isPending}
                onClick={() => save.mutate(draft)}
              >
                Save
              </button>
            </>
          )
        }
      />
      <Preview settings={draft} />
      <div className="grid two">
        <Card title="Crossfade">
          <div className="stack">
            <Field label="The next track starts">
              <Segmented
                label="The next track starts"
                value={crossfade.mode}
                disabled={disabled}
                onChange={(mode) => setCrossfade({ mode })}
                options={[
                  { value: "auto", label: "At the crossfade point" },
                  { value: "fixed", label: "A fixed time before the end" },
                ]}
              />
            </Field>
            <p className="muted small" style={{ margin: 0 }}>
              {crossfade.mode === "auto"
                ? "The analysis finds each track's crossfade point, after its last loud moment (see the analysis settings)."
                : "Every track hands over the same time before its cue-out."}
            </p>
            <div className="form-grid">
              {crossfade.mode === "fixed" && (
                <Field label="Before the cue-out">
                  <NumberInput
                    value={crossfade.fixed_ms / 1000}
                    min={0}
                    max={30}
                    step={0.1}
                    suffix="s"
                    disabled={disabled}
                    onChange={(s) => setCrossfade({ fixed_ms: Math.round(s * 1000) })}
                  />
                </Field>
              )}
              <Field label="No crossfade up to" hint="Jingles and IDs play back to back">
                <NumberInput
                  value={crossfade.min_track_ms / 1000}
                  min={0}
                  max={600}
                  step={1}
                  suffix="s"
                  disabled={disabled}
                  onChange={(s) => setCrossfade({ min_track_ms: Math.round(s * 1000) })}
                />
              </Field>
              <Field label="Fade on skip">
                <NumberInput
                  value={crossfade.skip_fade_ms / 1000}
                  min={0}
                  max={10}
                  step={0.1}
                  suffix="s"
                  disabled={disabled}
                  onChange={(s) => setCrossfade({ skip_fade_ms: Math.round(s * 1000) })}
                />
              </Field>
            </div>
            <fieldset>
              <legend>Fade-out</legend>
              <FadeEditor
                kind="out"
                value={crossfade.fade_out}
                disabled={disabled}
                onChange={(fade_out) => setCrossfade({ fade_out })}
              />
            </fieldset>
            <fieldset>
              <legend>Fade-in</legend>
              <FadeEditor
                kind="in"
                value={crossfade.fade_in}
                disabled={disabled}
                onChange={(fade_in) => setCrossfade({ fade_in })}
              />
            </fieldset>
          </div>
        </Card>
        <Card title="Loudness">
          <div className="stack">
            <label className="check">
              <input
                type="checkbox"
                checked={loudness.enabled}
                disabled={disabled}
                onChange={(event) => setLoudness({ enabled: event.target.checked })}
              />
              Bring every track to the same loudness (EBU R128)
            </label>
            <div className="form-grid">
              <Field label="Target" hint="-16 LUFS suits most streams; -14 is louder">
                <NumberInput
                  value={loudness.target_lufs}
                  min={-40}
                  max={0}
                  step={0.5}
                  suffix="LUFS"
                  disabled={disabled || !loudness.enabled}
                  onChange={(target_lufs) => setLoudness({ target_lufs })}
                />
              </Field>
              <Field label="Most boost" hint="For quiet tracks">
                <NumberInput
                  value={loudness.max_gain_db}
                  min={0}
                  max={30}
                  step={0.5}
                  suffix="dB"
                  disabled={disabled || !loudness.enabled}
                  onChange={(max_gain_db) => setLoudness({ max_gain_db })}
                />
              </Field>
              <Field label="Peak ceiling" hint="Boosts never push peaks above it">
                <NumberInput
                  value={loudness.peak_ceiling_dbtp}
                  min={-12}
                  max={0}
                  step={0.1}
                  suffix="dBTP"
                  disabled={disabled || !loudness.enabled}
                  onChange={(peak_ceiling_dbtp) => setLoudness({ peak_ceiling_dbtp })}
                />
              </Field>
            </div>
            <p className="muted small" style={{ margin: 0 }}>
              There is no limiter yet, so a boost that would clip is reduced instead.
            </p>
          </div>
        </Card>
      </div>
    </>
  );
}

/** The transition between two tracks with the settings being edited. */
function Preview({ settings }: { settings: PlayoutSettings }) {
  const player = useLive((live) => live.player);
  const [outgoing, setOutgoing] = useState<Pick | null>(null);
  const [incoming, setIncoming] = useState<Pick | null>(null);
  const [picking, setPicking] = useState<"outgoing" | "incoming" | null>(null);

  // By default, what is on air and what comes next; else two tracks.
  const fallback = useQuery({
    queryKey: ["tracks", "preview-pair"],
    queryFn: () =>
      unwrap(api.GET("/api/v1/tracks", { params: { query: { filter: "playable", sort: "added", limit: 2 } } })),
    enabled: outgoing === null || incoming === null,
  });
  useEffect(() => {
    if (outgoing && incoming) {
      return;
    }
    const onAir = player?.decks.find((deck) => deck.on_air && deck.deck !== "fx")?.entry?.track;
    const next = player?.cued?.track ?? player?.queue[0]?.track;
    const pick = (track: { track_id?: number | null } & Parameters<typeof trackName>[0]) =>
      track.track_id ? { id: track.track_id, name: trackName(track) } : null;
    const live = onAir && next ? [pick(onAir), pick(next)] : [null, null];
    const [a, b] = fallback.data?.items ?? [];
    setOutgoing((current) => current ?? live[0] ?? (a ? { id: a.id, name: trackName(a) } : null));
    setIncoming((current) => current ?? live[1] ?? (b ? { id: b.id, name: trackName(b) } : null));
  }, [player, fallback.data, outgoing, incoming]);

  const debounced = useDebounced(settings, 250);
  const preview = useQuery({
    queryKey: ["transition", outgoing?.id, incoming?.id, debounced],
    enabled: outgoing !== null && incoming !== null,
    queryFn: () =>
      unwrap(
        api.POST("/api/v1/playout/transition", {
          body: {
            from_track_id: outgoing?.id ?? 0,
            to_track_id: incoming?.id ?? 0,
            settings: debounced,
          },
        }),
      ),
    placeholderData: keepPreviousData,
  });
  const outgoingWave = useWaveform(outgoing?.id);
  const incomingWave = useWaveform(incoming?.id);

  const choose = (track: TrackSummary) => {
    const pick = { id: track.id, name: trackName(track) };
    if (picking === "outgoing") {
      setOutgoing(pick);
    } else {
      setIncoming(pick);
    }
    setPicking(null);
  };

  return (
    <Card
      title="Preview"
      actions={
        <span className="muted small">
          Updates as you change the settings. Nothing is saved or played.
        </span>
      }
    >
      <div className="stack">
        <div className="row">
          <button type="button" className="btn small" onClick={() => setPicking("outgoing")}>
            <span className="swatch" style={{ background: "var(--deck-a)" }} />
            <span className="ellipsis" style={{ maxWidth: "18rem" }}>
              {outgoing?.name ?? "Pick the outgoing track"}
            </span>
          </button>
          <span className="muted">into</span>
          <button type="button" className="btn small" onClick={() => setPicking("incoming")}>
            <span className="swatch" style={{ background: "var(--deck-b)" }} />
            <span className="ellipsis" style={{ maxWidth: "18rem" }}>
              {incoming?.name ?? "Pick the incoming track"}
            </span>
          </button>
          {preview.data && (
            <span className="muted small">
              Gains {db(preview.data.outgoing.gain_db)} and {db(preview.data.incoming.gain_db)}
            </span>
          )}
        </div>
        {preview.isError && <ErrorView error={preview.error} />}
        {!outgoing || !incoming ? (
          <Empty>Pick two analyzed tracks to see their transition.</Empty>
        ) : preview.data ? (
          <TransitionGraph
            preview={preview.data}
            outgoingWave={outgoingWave.data ?? null}
            incomingWave={incomingWave.data ?? null}
          />
        ) : (
          <Loading />
        )}
      </div>
      <Modal
        title={picking === "outgoing" ? "Outgoing track" : "Incoming track"}
        open={picking !== null}
        onClose={() => setPicking(null)}
      >
        <TrackSearch
          actions={(track) => (
            <button type="button" className="btn small primary" onClick={() => choose(track)}>
              Pick
            </button>
          )}
        />
      </Modal>
    </Card>
  );
}
