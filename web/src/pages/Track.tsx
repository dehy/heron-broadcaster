// SPDX-License-Identifier: AGPL-3.0-or-later

// One track: waveform with its cue and fade points, preview, per-track
// overrides, categories, analysis and plays.

import { useQuery } from "@tanstack/react-query";
import { useEffect, useRef, useState } from "react";
import {
  type Fade,
  type PlayoutSettings,
  type TrackDetail,
  type TrackOverrides,
  api,
  done,
  unwrap,
} from "../api";
import { FadeEditor } from "../components/FadeEditor";
import { type Marker, type Region, WaveformView, useWaveform } from "../components/Waveform";
import {
  Banner,
  Card,
  Empty,
  ErrorView,
  Field,
  Icon,
  Loading,
  NumberInput,
  PageHead,
  Segmented,
} from "../components/ui";
import { ago, bytes, dateTime, db, duration, parseDuration, trackName } from "../lib/format";
import { Link } from "../lib/router";
import { useAction, useCan } from "../lib/session";

/** Whether overrides change anything (every field unset). */
function isDefault(overrides: TrackOverrides): boolean {
  const unset = (value: unknown) => value === null || value === undefined;
  return (
    unset(overrides.cue_in_ms) &&
    unset(overrides.cue_out_ms) &&
    unset(overrides.gap_killer_enabled) &&
    Object.values(overrides.crossfade ?? {}).every(unset)
  );
}

/** The same overrides, with unset fields as `null`, so drafts compare. */
function normalize(overrides: TrackOverrides): TrackOverrides {
  const cf = overrides.crossfade ?? {};
  return {
    cue_in_ms: overrides.cue_in_ms ?? null,
    cue_out_ms: overrides.cue_out_ms ?? null,
    gap_killer_enabled: overrides.gap_killer_enabled ?? null,
    crossfade: {
      crossfade: cf.crossfade ?? null,
      fade_in: cf.fade_in ?? null,
      fade_out: cf.fade_out ?? null,
      fixed_ms: cf.fixed_ms ?? null,
      mode: cf.mode ?? null,
    },
  };
}

export function Track({ id }: { id: number }) {
  const track = useQuery({
    queryKey: ["track", id],
    queryFn: () => unwrap(api.GET("/api/v1/tracks/{id}", { params: { path: { id } } })),
  });
  const playout = useQuery({
    queryKey: ["settings", "playout"],
    queryFn: () => unwrap(api.GET("/api/v1/settings/playout")),
  });
  if (track.isPending) {
    return <Loading />;
  }
  if (track.isError) {
    return <ErrorView error={track.error} />;
  }
  return <TrackView track={track.data} settings={playout.data} />;
}

/** Where the track plays from and to, with `overrides` applied. */
export function effectiveCues(track: TrackDetail, overrides: TrackOverrides) {
  const length = track.duration_ms ?? track.analysis?.cue_out_ms ?? 0;
  let cueIn = track.analysis?.cue_in_ms ?? 0;
  let cueOut = track.analysis?.cue_out_ms ?? length;
  if (overrides.gap_killer_enabled === false) {
    cueIn = 0;
    cueOut = length;
  }
  cueIn = overrides.cue_in_ms ?? cueIn;
  cueOut = Math.min(overrides.cue_out_ms ?? cueOut, length || Number.POSITIVE_INFINITY);
  return { length, cueIn: Math.min(cueIn, cueOut), cueOut };
}

function TrackView({ track, settings }: { track: TrackDetail; settings: PlayoutSettings | undefined }) {
  const canEdit = useCan("operator");
  const waveform = useWaveform(track.id);
  const saved = normalize(track.overrides);
  const [draft, setDraft] = useState<TrackOverrides>(saved);
  const [zoom, setZoom] = useState<"whole" | "intro" | "outro">("whole");
  const audio = useRef<HTMLAudioElement>(null);
  const [playing, setPlaying] = useState(false);
  const [playhead, setPlayhead] = useState<number | null>(null);

  useEffect(() => {
    setDraft(normalize(track.overrides));
  }, [track.overrides]);

  // Follow the audio while it plays.
  useEffect(() => {
    if (!playing) {
      return;
    }
    let frame = 0;
    const step = () => {
      if (audio.current) {
        setPlayhead(audio.current.currentTime * 1000);
      }
      frame = requestAnimationFrame(step);
    };
    frame = requestAnimationFrame(step);
    return () => cancelAnimationFrame(frame);
  }, [playing]);

  const saveOverrides = useAction(
    (overrides: TrackOverrides) =>
      unwrap(api.PUT("/api/v1/tracks/{id}/overrides", { params: { path: { id: track.id } }, body: overrides })),
    { success: "Saved: queued entries follow within seconds", invalidate: [["track", track.id]] },
  );
  const clearOverrides = useAction(
    () => done(api.DELETE("/api/v1/tracks/{id}/overrides", { params: { path: { id: track.id } } })),
    { success: "Back to the analysis and station settings", invalidate: [["track", track.id]] },
  );
  const setEnabled = useAction(
    (enabled: boolean) =>
      unwrap(api.PATCH("/api/v1/tracks/{id}", { params: { path: { id: track.id } }, body: { enabled } })),
    { invalidate: [["track", track.id], ["tracks"]] },
  );
  const enqueue = useAction(
    (next: boolean) =>
      unwrap(
        api.POST("/api/v1/queue", {
          body: next ? { track_ids: [track.id], position: 0 } : { track_ids: [track.id] },
        }),
      ),
    { success: "Queued" },
  );

  const { length, cueIn, cueOut } = effectiveCues(track, draft);
  const crossfade = settings?.crossfade;
  const overrides = draft.crossfade;
  const mode = overrides.mode ?? crossfade?.mode ?? "auto";
  const fixedMs = overrides.fixed_ms ?? crossfade?.fixed_ms ?? 2000;
  const shortTrack = crossfade ? cueOut - cueIn <= crossfade.min_track_ms : false;
  const crossfades = overrides.crossfade !== false && !shortTrack;
  const autoPoint = track.analysis?.fade_out_start_ms ?? null;
  const mixPoint = !crossfades
    ? null
    : mode === "fixed"
      ? Math.max(cueIn, cueOut - fixedMs)
      : autoPoint !== null && autoPoint >= cueIn && autoPoint < cueOut
        ? autoPoint
        : null;
  const fadeOut: Fade | undefined = overrides.fade_out ?? crossfade?.fade_out;
  const fadeIn: Fade | undefined = overrides.fade_in ?? crossfade?.fade_in;

  const dirty = JSON.stringify(draft) !== JSON.stringify(saved);
  const hasOverrides = !isDefault(track.overrides);

  const markers: Marker[] = [
    {
      id: "cue_in",
      label: "Cue in",
      ms: cueIn,
      color: "var(--good)",
      ...(canEdit && {
        onDrag: (ms: number) => setDraft((d) => ({ ...d, cue_in_ms: Math.min(ms, cueOut - 100) })),
      }),
    },
    {
      id: "cue_out",
      label: "Cue out",
      ms: cueOut,
      color: "var(--bad)",
      end: true,
      ...(canEdit && {
        onDrag: (ms: number) => setDraft((d) => ({ ...d, cue_out_ms: Math.max(ms, cueIn + 100) })),
      }),
    },
  ];
  if (mixPoint !== null) {
    markers.push({
      id: "mix",
      label: "Next starts",
      ms: mixPoint,
      color: "var(--accent)",
      end: true,
      row: 1,
      ...(canEdit && {
        // Dragging it sets a fixed crossfade for this track.
        onDrag: (ms: number) =>
          setDraft((d) => ({
            ...d,
            crossfade: {
              ...d.crossfade,
              mode: "fixed",
              fixed_ms: Math.max(0, Math.round((cueOut - Math.min(ms, cueOut)) / 100) * 100),
            },
          })),
      }),
    });
  }
  const regions: Region[] = [];
  if (mixPoint !== null && fadeOut?.enabled) {
    regions.push({
      startMs: mixPoint,
      endMs: Math.min(cueOut, mixPoint + fadeOut.time_ms),
      color: getVar("--bad"),
      label: "Fade-out",
    });
  }
  if (fadeIn?.enabled && crossfades) {
    regions.push({
      startMs: cueIn,
      endMs: Math.min(cueOut, cueIn + fadeIn.time_ms),
      color: getVar("--good"),
      label: "Fade-in",
    });
  }

  const view: [number, number] | undefined =
    zoom === "intro"
      ? [Math.max(0, cueIn - 5_000), Math.min(length, cueIn + 20_000)]
      : zoom === "outro"
        ? [Math.max(0, (mixPoint ?? cueOut) - 15_000), Math.min(length, cueOut + 5_000)]
        : undefined;

  const playFrom = (ms: number) => {
    const element = audio.current;
    if (!element) {
      return;
    }
    element.currentTime = ms / 1000;
    setPlayhead(ms);
    void element.play();
  };

  return (
    <>
      <PageHead
        title={track.title ?? trackName(track)}
        description={
          <>
            {[track.artist, track.album].filter(Boolean).join(" · ") || "No tags"} ·{" "}
            <code>{track.path}</code>
          </>
        }
        actions={
          canEdit && (
            <>
              {!track.missing && !track.silent && (
                <>
                  <button type="button" className="btn" onClick={() => enqueue.mutate(true)}>
                    <Icon name="next" /> Play next
                  </button>
                  <button type="button" className="btn" onClick={() => enqueue.mutate(false)}>
                    <Icon name="plus" /> Queue
                  </button>
                </>
              )}
              <button
                type="button"
                className="btn"
                onClick={() => setEnabled.mutate(!track.enabled)}
                title={track.enabled ? "Never picked by autofill or the rotation" : undefined}
              >
                {track.enabled ? "Disable" : "Enable"}
              </button>
            </>
          )
        }
      />
      {track.missing && (
        <Banner kind="bad">
          The file has been missing since {dateTime(track.missing_since_unix_ms)}: the track is not
          played.
        </Banner>
      )}
      {track.silent && <Banner kind="bad">The analysis found nothing audible: the track is not played.</Banner>}
      {!track.enabled && <Banner kind="warn">Disabled: autofill and the rotation never pick it.</Banner>}
      {track.identification?.status === "needs_review" && (
        <Banner kind="warn">
          Its identification needs review ({track.identification.reason}).{" "}
          <Link to="/review">Review it</Link>
        </Banner>
      )}

      <Card
        title="Waveform"
        actions={
          <>
            <Segmented
              label="Zoom"
              value={zoom}
              onChange={setZoom}
              options={[
                { value: "whole", label: "Whole" },
                { value: "intro", label: "Intro" },
                { value: "outro", label: "Outro" },
              ]}
            />
            {!track.missing && (
              <>
                <button
                  type="button"
                  className="btn small"
                  onClick={() => (playing ? audio.current?.pause() : playFrom(playhead ?? cueIn))}
                >
                  <Icon name={playing ? "pause" : "play"} size={14} /> {playing ? "Pause" : "Listen"}
                </button>
                <button type="button" className="btn small" onClick={() => playFrom(cueIn)}>
                  From the cue-in
                </button>
                <button
                  type="button"
                  className="btn small"
                  onClick={() => playFrom(Math.max(0, (mixPoint ?? cueOut) - 5_000))}
                >
                  The ending
                </button>
              </>
            )}
          </>
        }
      >
        <div className="stack">
          {waveform.isError ? (
            <ErrorView error={waveform.error} />
          ) : (
            <WaveformView
              data={waveform.data ?? null}
              durationMs={length}
              markers={markers}
              regions={regions}
              playheadMs={playhead}
              onSeek={playFrom}
              {...(view && { view })}
            />
          )}
          <div className="legend muted">
            <span>
              <span className="swatch" style={{ background: "var(--good)" }} /> Cue in{" "}
              <strong className="num">{duration(cueIn, true)}</strong>
            </span>
            <span>
              <span className="swatch" style={{ background: "var(--bad)" }} /> Cue out{" "}
              <strong className="num">{duration(cueOut, true)}</strong>
            </span>
            <span>
              <span className="swatch" style={{ background: "var(--accent)" }} /> Next track starts{" "}
              <strong className="num">
                {mixPoint !== null ? duration(mixPoint, true) : "after the cue-out"}
              </strong>
            </span>
            <span>Plays {duration(cueOut - cueIn)}</span>
            {canEdit && <span>Drag the flags to change the points, then save.</span>}
          </div>
          <audio
            ref={audio}
            preload="none"
            src={`/api/v1/tracks/${track.id}/audio`}
            onPlay={() => setPlaying(true)}
            onPause={() => setPlaying(false)}
            onEnded={() => setPlaying(false)}
          />
        </div>
      </Card>

      <div className="grid two">
        <OverridesCard
          track={track}
          settings={settings}
          draft={draft}
          setDraft={setDraft}
          canEdit={canEdit}
          dirty={dirty}
          hasOverrides={hasOverrides}
          shortTrack={shortTrack}
          onSave={() => saveOverrides.mutate(draft)}
          onRevert={() => setDraft(saved)}
          onClear={() => clearOverrides.mutate(undefined)}
          saving={saveOverrides.isPending}
        />
        <div className="stack">
          <CategoriesCard track={track} canEdit={canEdit} />
          <DetailsCard track={track} />
          <PlaysCard trackId={track.id} />
        </div>
      </div>
    </>
  );
}

function getVar(name: string): string {
  return getComputedStyle(document.documentElement).getPropertyValue(name).trim() || "#888";
}

/** A time typed as `m:ss.t`, or empty for the default. */
function CueInput({
  value,
  placeholder,
  onChange,
  disabled,
}: {
  value: number | null;
  placeholder: number;
  onChange: (ms: number | null) => void;
  disabled: boolean;
}) {
  const [text, setText] = useState(value === null ? "" : duration(value, true));
  useEffect(() => {
    setText(value === null ? "" : duration(value, true));
  }, [value]);
  const valid = text.trim() === "" || parseDuration(text) !== null;
  return (
    <input
      className={valid ? "input num" : "input num invalid"}
      value={text}
      placeholder={duration(placeholder, true)}
      disabled={disabled}
      aria-invalid={!valid}
      onChange={(event) => setText(event.target.value)}
      onBlur={() => {
        if (text.trim() === "") {
          onChange(null);
        } else {
          const ms = parseDuration(text);
          if (ms !== null) {
            onChange(ms);
          }
        }
      }}
    />
  );
}

type Tri = "default" | "on" | "off";
const tri = (value: boolean | null): Tri => (value === null ? "default" : value ? "on" : "off");
const fromTri = (value: Tri): boolean | null => (value === "default" ? null : value === "on");

function OverridesCard({
  track,
  settings,
  draft,
  setDraft,
  canEdit,
  dirty,
  hasOverrides,
  shortTrack,
  onSave,
  onRevert,
  onClear,
  saving,
}: {
  track: TrackDetail;
  settings: PlayoutSettings | undefined;
  draft: TrackOverrides;
  setDraft: (update: (draft: TrackOverrides) => TrackOverrides) => void;
  canEdit: boolean;
  dirty: boolean;
  hasOverrides: boolean;
  shortTrack: boolean;
  onSave: () => void;
  onRevert: () => void;
  onClear: () => void;
  saving: boolean;
}) {
  const disabled = !canEdit;
  const analysis = track.analysis;
  const station = settings?.crossfade;
  const cf = draft.crossfade;
  const setCrossfade = (changes: Partial<TrackOverrides["crossfade"]>) =>
    setDraft((d) => ({ ...d, crossfade: { ...d.crossfade, ...changes } }));
  return (
    <Card
      title="Playout of this track"
      actions={
        canEdit && (
          <>
            {hasOverrides && !dirty && (
              <button type="button" className="btn small" onClick={onClear}>
                <Icon name="undo" size={14} /> Use the defaults
              </button>
            )}
            {dirty && (
              <button type="button" className="btn small" onClick={onRevert}>
                Revert
              </button>
            )}
            <button type="button" className="btn small primary" disabled={!dirty || saving} onClick={onSave}>
              Save
            </button>
          </>
        )
      }
    >
      <div className="stack">
        <p className="muted small" style={{ margin: 0 }}>
          Empty fields use the analysis and the station's{" "}
          <Link to="/playout">crossfade settings</Link>.
        </p>
        <div className="form-grid">
          <Field label="Cue in" hint={`Analysis: ${duration(analysis?.cue_in_ms ?? 0, true)}`}>
            <CueInput
              value={draft.cue_in_ms}
              placeholder={analysis?.cue_in_ms ?? 0}
              disabled={disabled}
              onChange={(ms) => setDraft((d) => ({ ...d, cue_in_ms: ms }))}
            />
          </Field>
          <Field
            label="Cue out"
            hint={`Analysis: ${duration(analysis?.cue_out_ms ?? track.duration_ms ?? 0, true)}`}
          >
            <CueInput
              value={draft.cue_out_ms}
              placeholder={analysis?.cue_out_ms ?? track.duration_ms ?? 0}
              disabled={disabled}
              onChange={(ms) => setDraft((d) => ({ ...d, cue_out_ms: ms }))}
            />
          </Field>
          <Field label="Gap Killer" hint="Off plays the whole file">
            <select
              className="input"
              value={tri(draft.gap_killer_enabled)}
              disabled={disabled}
              onChange={(event) =>
                setDraft((d) => ({ ...d, gap_killer_enabled: fromTri(event.target.value as Tri) }))
              }
            >
              <option value="default">Station default (on)</option>
              <option value="on">On</option>
              <option value="off">Off</option>
            </select>
          </Field>
        </div>

        <fieldset>
          <legend>Into the next track</legend>
          <div className="stack">
            {shortTrack && (
              <p className="muted small" style={{ margin: 0 }}>
                Shorter than {duration(station?.min_track_ms ?? 0)}: played back to back, never
                crossfaded.
              </p>
            )}
            <div className="form-grid">
              <Field label="Crossfade">
                <select
                  className="input"
                  value={cf.crossfade === false ? "off" : "default"}
                  disabled={disabled}
                  onChange={(event) =>
                    setCrossfade({ crossfade: event.target.value === "off" ? false : null })
                  }
                >
                  <option value="default">Crossfade</option>
                  <option value="off">Back to back</option>
                </select>
              </Field>
              <Field label="Next track starts">
                <select
                  className="input"
                  value={cf.mode ?? "default"}
                  disabled={disabled || cf.crossfade === false}
                  onChange={(event) =>
                    setCrossfade({
                      mode: event.target.value === "default" ? null : (event.target.value as "auto" | "fixed"),
                    })
                  }
                >
                  <option value="default">Station default ({station?.mode === "fixed" ? "fixed" : "automatic"})</option>
                  <option value="auto">At the analysis' crossfade point</option>
                  <option value="fixed">A fixed time before the cue-out</option>
                </select>
              </Field>
              {(cf.mode ?? station?.mode) === "fixed" && (
                <Field label="Before the cue-out">
                  <NumberInput
                    value={(cf.fixed_ms ?? station?.fixed_ms ?? 2000) / 1000}
                    min={0}
                    max={30}
                    step={0.1}
                    suffix="s"
                    disabled={disabled || cf.crossfade === false}
                    onChange={(seconds) => setCrossfade({ fixed_ms: Math.round(seconds * 1000) })}
                  />
                </Field>
              )}
            </div>
            <OverrideFade
              label="Fade-out of this track"
              kind="out"
              value={cf.fade_out}
              station={station?.fade_out}
              disabled={disabled || cf.crossfade === false}
              onChange={(fade) => setCrossfade({ fade_out: fade })}
            />
          </div>
        </fieldset>
        <fieldset>
          <legend>Out of the previous track</legend>
          <OverrideFade
            label="Fade-in of this track"
            kind="in"
            value={cf.fade_in}
            station={station?.fade_in}
            disabled={disabled}
            onChange={(fade) => setCrossfade({ fade_in: fade })}
          />
        </fieldset>
      </div>
    </Card>
  );
}

function OverrideFade({
  label,
  kind,
  value,
  station,
  disabled,
  onChange,
}: {
  label: string;
  kind: "in" | "out";
  value: Fade | null;
  station: Fade | undefined;
  disabled: boolean;
  onChange: (fade: Fade | null) => void;
}) {
  return (
    <div className="stack">
      <label className="check">
        <input
          type="checkbox"
          checked={value !== null}
          disabled={disabled || !station}
          onChange={(event) => onChange(event.target.checked && station ? { ...station } : null)}
        />
        {label}: {value === null ? "station setting" : "set for this track"}
      </label>
      {value !== null && <FadeEditor value={value} onChange={onChange} kind={kind} disabled={disabled} />}
    </div>
  );
}

function CategoriesCard({ track, canEdit }: { track: TrackDetail; canEdit: boolean }) {
  const categories = useQuery({
    queryKey: ["categories"],
    queryFn: () => unwrap(api.GET("/api/v1/categories")),
  });
  const save = useAction(
    (assigned: { category_id: number; weight: number }[]) =>
      unwrap(api.PUT("/api/v1/tracks/{id}/categories", { params: { path: { id: track.id } }, body: assigned })),
    { invalidate: [["track", track.id], ["categories"]] },
  );
  const current = track.categories.map((c) => ({ category_id: c.category_id, weight: c.weight }));
  const available = (categories.data ?? []).filter(
    (c) => !track.categories.some((assigned) => assigned.category_id === c.id),
  );
  return (
    <Card title="Categories" actions={<Link to="/categories" className="btn small">Manage</Link>}>
      <div className="stack">
        {track.categories.length === 0 && (
          <p className="muted" style={{ margin: 0 }}>
            In no category.
          </p>
        )}
        <ul className="list">
          {track.categories.map((category) => (
            <li key={category.category_id} className="row between" style={{ padding: "0.35rem 0" }}>
              <span className="row">
                <span className="swatch" style={{ background: category.color ?? undefined }} />
                {category.name}
              </span>
              <span className="row">
                <label className="row small muted" title="Chance of being picked within the category">
                  Weight
                  <span style={{ width: "5rem" }}>
                    <NumberInput
                      value={category.weight}
                      min={0.01}
                      step={0.1}
                      disabled={!canEdit}
                      onChange={(weight) =>
                        save.mutate(
                          current.map((c) =>
                            c.category_id === category.category_id ? { ...c, weight } : c,
                          ),
                        )
                      }
                    />
                  </span>
                </label>
                {canEdit && (
                  <button
                    type="button"
                    className="btn ghost small icon danger"
                    aria-label={`Remove from ${category.name}`}
                    onClick={() =>
                      save.mutate(current.filter((c) => c.category_id !== category.category_id))
                    }
                  >
                    <Icon name="close" />
                  </button>
                )}
              </span>
            </li>
          ))}
        </ul>
        {canEdit && available.length > 0 && (
          <label>
            <span className="visually-hidden">Add to a category</span>
            <select
              className="input"
              value=""
              onChange={(event) =>
                save.mutate([...current, { category_id: Number(event.target.value), weight: 1 }])
              }
            >
              <option value="">Add to a category…</option>
              {available.map((category) => (
                <option key={category.id} value={category.id}>
                  {category.name}
                </option>
              ))}
            </select>
          </label>
        )}
      </div>
    </Card>
  );
}

function DetailsCard({ track }: { track: TrackDetail }) {
  const analysis = track.analysis;
  return (
    <Card title="Analysis and file">
      <dl className="facts small">
        <dt>Loudness</dt>
        <dd>{analysis ? db(analysis.integrated_lufs, "LUFS") : "Not analyzed yet"}</dd>
        <dt>True peak</dt>
        <dd>{analysis ? db(analysis.true_peak_db, "dBTP") : "–"}</dd>
        <dt>Normalization</dt>
        <dd>{db(track.playout.gain_db)}</dd>
        {analysis?.has_internal_silence && (
          <>
            <dt>Silence</dt>
            <dd className="warn">A long silence inside: the track ends there.</dd>
          </>
        )}
        <dt>Analyzed</dt>
        <dd>{analysis ? dateTime(analysis.analyzed_at_unix_ms) : "–"}</dd>
        <dt>Length</dt>
        <dd className="num">{duration(track.duration_ms, true)}</dd>
        <dt>Size</dt>
        <dd>{bytes(track.file_size)}</dd>
        <dt>Added</dt>
        <dd>{dateTime(track.added_at_unix_ms)}</dd>
        <dt>Identification</dt>
        <dd>
          {track.identification?.status === "identified" && track.identification.release_mbid ? (
            <a
              href={`https://musicbrainz.org/release/${track.identification.release_mbid}`}
              target="_blank"
              rel="noreferrer"
            >
              MusicBrainz release
            </a>
          ) : track.identification?.status === "needs_review" ? (
            <span className="warn">Needs review</span>
          ) : track.identification?.status === "kept" ? (
            "Reviewed: its own tags"
          ) : (
            "Its own tags"
          )}
        </dd>
      </dl>
    </Card>
  );
}

function PlaysCard({ trackId }: { trackId: number }) {
  const plays = useQuery({
    queryKey: ["history", "track", trackId],
    queryFn: () =>
      unwrap(api.GET("/api/v1/history", { params: { query: { track_id: trackId, limit: 10 } } })),
  });
  return (
    <Card title="Recent plays" flush>
      {plays.isError && <ErrorView error={plays.error} />}
      {plays.data?.length === 0 && <Empty>Never played.</Empty>}
      <ul className="list">
        {plays.data?.map((play) => (
          <li key={play.id} className="queue-item">
            <span className="meta">{dateTime(play.started_at_unix_ms)}</span>
            <span className="muted small">
              {play.end_reason ?? (play.ended_at_unix_ms ? "" : "playing")}
              {play.played_ms ? ` · ${duration(play.played_ms)}` : ""} · {ago(play.started_at_unix_ms)}
            </span>
          </li>
        ))}
      </ul>
    </Card>
  );
}
