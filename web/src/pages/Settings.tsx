// SPDX-License-Identifier: AGPL-3.0-or-later

// Station settings: autofill, rotation, time zone, analysis and library
// management.

import { useQuery } from "@tanstack/react-query";
import { type ReactNode, useEffect, useState } from "react";
import { type Schemas, api, unwrap } from "../api";
import { Card, ErrorView, Field, Loading, NumberInput, PageHead } from "../components/ui";
import { WINDOW_FIELDS } from "../lib/rotation";
import { Link } from "../lib/router";
import { useCategories } from "./Clocks";
import { useAction, useCan } from "../lib/session";

export function Settings() {
  return (
    <>
      <PageHead title="Settings" description="How the station fills its queue, analyzes and files its music." />
      <QueueCard />
      <RotationCard />
      <StationCard />
      <LibraryCard />
      <AnalysisCard />
    </>
  );
}

/** A card editing one settings document, saved as a whole. */
function SettingsCard<T>({
  title,
  description,
  value,
  save,
  saving,
  canEdit,
  children,
}: {
  title: string;
  description?: ReactNode;
  value: T;
  save: (value: T) => void;
  saving: boolean;
  canEdit: boolean;
  children: (draft: T, set: (changes: Partial<T>) => void) => ReactNode;
}) {
  const [draft, setDraft] = useState(value);
  useEffect(() => setDraft(value), [value]);
  const dirty = JSON.stringify(draft) !== JSON.stringify(value);
  return (
    <Card
      title={title}
      actions={
        canEdit && (
          <>
            {dirty && (
              <button type="button" className="btn small" onClick={() => setDraft(value)}>
                Revert
              </button>
            )}
            <button
              type="button"
              className="btn small primary"
              disabled={!dirty || saving}
              onClick={() => save(draft)}
            >
              Save
            </button>
          </>
        )
      }
    >
      <div className="stack">
        {description && (
          <p className="muted small" style={{ margin: 0 }}>
            {description}
          </p>
        )}
        {children(draft, (changes) => setDraft((d) => ({ ...d, ...changes })))}
      </div>
    </Card>
  );
}

function QueueCard() {
  const canEdit = useCan("operator");
  const settings = useQuery({
    queryKey: ["settings", "queue"],
    queryFn: () => unwrap(api.GET("/api/v1/settings/queue")),
  });
  const save = useAction(
    (body: Schemas["QueueSettings"]) => unwrap(api.PUT("/api/v1/settings/queue", { body })),
    { success: "Saved", invalidate: [["settings", "queue"]] },
  );
  if (settings.isPending) {
    return <Loading />;
  }
  if (settings.isError) {
    return <ErrorView error={settings.error} />;
  }
  return (
    <SettingsCard
      title="Queue"
      description={
        <>
          Autofill queues the tracks rotation picks, following the <Link to="/schedule">schedule</Link> and its{" "}
          <Link to="/clocks">clocks</Link>. Every pick goes to the <Link to="/rotation">selection log</Link>.
        </>
      }
      value={settings.data}
      save={(value) => save.mutate(value)}
      saving={save.isPending}
      canEdit={canEdit}
    >
      {(draft, set) => (
        <div className="form-grid">
          <label className="check">
            <input
              type="checkbox"
              checked={draft.autofill}
              disabled={!canEdit}
              onChange={(e) => set({ autofill: e.target.checked })}
            />
            Autofill
          </label>
          <Field label="Tracks kept waiting" hint="The cued one included, 1 to 50">
            <NumberInput
              value={draft.min_queued}
              min={1}
              max={50}
              step={1}
              disabled={!canEdit || !draft.autofill}
              onChange={(min_queued) => set({ min_queued: Math.round(min_queued) })}
            />
          </Field>
        </div>
      )}
    </SettingsCard>
  );
}

const WINDOW_LABELS: Record<string, string> = {
  album_min: "Same album within",
  artist_min: "Same artist within",
  title_min: "Same title within",
  track_min: "Same track within",
};

function RotationCard() {
  const canEdit = useCan("operator");
  const categories = useCategories();
  const settings = useQuery({
    queryKey: ["settings", "rotation"],
    queryFn: () => unwrap(api.GET("/api/v1/settings/rotation")),
  });
  const save = useAction(
    (body: Schemas["RotationSettings"]) => unwrap(api.PUT("/api/v1/settings/rotation", { body })),
    { success: "Saved", invalidate: [["settings", "rotation"]] },
  );
  if (settings.isPending) {
    return <Loading />;
  }
  if (settings.isError) {
    return <ErrorView error={settings.error} />;
  }
  return (
    <SettingsCard
      title="Rotation"
      description="Separation windows, in minutes (0 turns a rule off); categories and clock slots can override them. When no track fits, rules are relaxed step by step: album, artist, title, then track separation, each halved before being dropped, then the fallback category."
      value={settings.data}
      save={(value) => save.mutate(value)}
      saving={save.isPending}
      canEdit={canEdit}
    >
      {(draft, set) => (
        <>
          <div className="form-grid">
            {WINDOW_FIELDS.map(([key]) => (
              <Field key={key} label={WINDOW_LABELS[key]}>
                <NumberInput
                  value={draft.separation[key]}
                  min={0}
                  max={10_080}
                  step={1}
                  suffix="min"
                  disabled={!canEdit}
                  onChange={(minutes) => set({ separation: { ...draft.separation, [key]: Math.round(minutes) } })}
                />
              </Field>
            ))}
          </div>
          <div className="form-grid">
            <Field label="Fallback category" hint="Where relaxation ends: a large, safe category">
              <select
                className="input"
                value={draft.fallback_category_id ?? ""}
                disabled={!canEdit}
                onChange={(e) => set({ fallback_category_id: e.target.value ? Number(e.target.value) : null })}
              >
                <option value="">The whole library</option>
                {categories.data?.map((category) => (
                  <option key={category.id} value={category.id}>
                    {category.name}
                  </option>
                ))}
              </select>
            </Field>
            <Field label="Weight after a play" hint="Multiplies the track's weights, 0.5 to 1 (1: unchanged)">
              <NumberInput
                value={draft.weights.play_factor}
                min={0.5}
                max={1}
                step={0.05}
                disabled={!canEdit}
                onChange={(play_factor) => set({ weights: { ...draft.weights, play_factor } })}
              />
            </Field>
            <Field label="Weight after a request" hint="1 to 2 (1: unchanged)">
              <NumberInput
                value={draft.weights.request_factor}
                min={1}
                max={2}
                step={0.05}
                disabled={!canEdit}
                onChange={(request_factor) => set({ weights: { ...draft.weights, request_factor } })}
              />
            </Field>
          </div>
        </>
      )}
    </SettingsCard>
  );
}

function timeZones(): string[] {
  try {
    return Intl.supportedValuesOf("timeZone");
  } catch {
    return [];
  }
}

function StationCard() {
  const isAdmin = useCan("admin");
  const settings = useQuery({
    queryKey: ["settings", "station"],
    queryFn: () => unwrap(api.GET("/api/v1/settings/station")),
  });
  const save = useAction(
    (body: Schemas["StationSettings"]) => unwrap(api.PUT("/api/v1/settings/station", { body })),
    { success: "Saved", invalidate: [["settings", "station"]] },
  );
  if (settings.isPending) {
    return <Loading />;
  }
  if (settings.isError) {
    return <ErrorView error={settings.error} />;
  }
  return (
    <SettingsCard
      title="Station"
      description="The schedule, the restrictions and every time shown follow the station's time zone. When the clocks go back, the repeated hour runs its clock again; when they go forward, the skipped hour is skipped."
      value={settings.data}
      save={(value) => save.mutate(value)}
      saving={save.isPending}
      canEdit={isAdmin}
    >
      {(draft, set) => (
        <Field label="Time zone" hint="An IANA name, such as Europe/Paris">
          <input
            className="input"
            list="time-zones"
            value={draft.timezone}
            disabled={!isAdmin}
            onChange={(e) => set({ timezone: e.target.value })}
          />
          <datalist id="time-zones">
            {timeZones().map((zone) => (
              <option key={zone} value={zone} />
            ))}
          </datalist>
        </Field>
      )}
    </SettingsCard>
  );
}

function LibraryCard() {
  const isAdmin = useCan("admin");
  const settings = useQuery({
    queryKey: ["settings", "library"],
    queryFn: () => unwrap(api.GET("/api/v1/settings/library")),
  });
  const [key, setKey] = useState("");
  const save = useAction(
    (body: Schemas["LibrarySettingsPatch"]) => unwrap(api.PATCH("/api/v1/settings/library", { body })),
    { success: "Saved", invalidate: [["settings", "library"]], onSuccess: () => setKey("") },
  );
  if (settings.isPending) {
    return <Loading />;
  }
  if (settings.isError) {
    return <ErrorView error={settings.error} />;
  }
  const current = settings.data;
  return (
    <>
      <SettingsCard
        title="Library"
        description={
          <>
            Files dropped into the incoming folder are identified, tagged and filed by heron-librarian.
            Fields: <code>{"{album_artist} {artist} {album} {title} {year} {track:02} {disc} {disc-} {ext}"}</code>
          </>
        }
        value={{
          layout: current.layout,
          min_score: current.min_score,
          save_cover: current.save_cover,
          embed_cover: current.embed_cover,
        }}
        save={(value) => save.mutate(value)}
        saving={save.isPending}
        canEdit={isAdmin}
      >
        {(draft, set) => (
          <>
            <Field label="Folder layout" hint="Relative to the library root. Used for new files and for organizing.">
              <input
                className="input mono"
                value={draft.layout}
                disabled={!isAdmin}
                onChange={(e) => set({ layout: e.target.value })}
              />
            </Field>
            <div className="form-grid">
              <Field label="Lowest AcoustID score" hint="Matches below it wait for review">
                <NumberInput
                  value={draft.min_score}
                  min={0}
                  max={1}
                  step={0.01}
                  disabled={!isAdmin}
                  onChange={(min_score) => set({ min_score })}
                />
              </Field>
              <label className="check">
                <input
                  type="checkbox"
                  checked={draft.save_cover}
                  disabled={!isAdmin}
                  onChange={(e) => set({ save_cover: e.target.checked })}
                />
                Save the cover as cover.jpg
              </label>
              <label className="check">
                <input
                  type="checkbox"
                  checked={draft.embed_cover}
                  disabled={!isAdmin}
                  onChange={(e) => set({ embed_cover: e.target.checked })}
                />
                Embed the cover in files
              </label>
            </div>
          </>
        )}
      </SettingsCard>
      <Card title="Identification">
        <div className="stack">
          <p className="muted small" style={{ margin: 0 }}>
            Identification uses the station's own AcoustID application key (free keys are for
            non-commercial use). Without one, files are filed using their own tags.
          </p>
          <p style={{ margin: 0 }}>
            {current.acoustid_key_set ? (
              <span className="good">A key is set: identification is on.</span>
            ) : (
              <span className="warn">No key: identification is off.</span>
            )}
          </p>
          {isAdmin && (
            <div className="row">
              <label style={{ flex: "1 1 16rem" }}>
                <span className="visually-hidden">AcoustID key</span>
                <input
                  className="input mono"
                  type="password"
                  autoComplete="off"
                  placeholder={current.acoustid_key_set ? "Replace the key" : "AcoustID application key"}
                  value={key}
                  onChange={(e) => setKey(e.target.value)}
                />
              </label>
              <button
                type="button"
                className="btn primary"
                disabled={!key.trim() || save.isPending}
                onClick={() => save.mutate({ acoustid_key: key.trim() })}
              >
                Save the key
              </button>
              {current.acoustid_key_set && (
                <button type="button" className="btn danger" onClick={() => save.mutate({ acoustid_key: null })}>
                  Remove
                </button>
              )}
            </div>
          )}
        </div>
      </Card>
    </>
  );
}

const ANALYSIS_FIELDS: {
  key: keyof Schemas["AnalysisSettings"];
  label: string;
  hint: string;
  unit: string;
  min: number;
  max: number;
  step: number;
  ms?: boolean;
}[] = [
  { key: "start_threshold_db", label: "Start threshold", hint: "Below the track's loudness, trimmed at the start", unit: "dB", min: -80, max: 0, step: 1 },
  { key: "end_threshold_db", label: "End threshold", hint: "Below the track's loudness, trimmed at the end", unit: "dB", min: -80, max: 0, step: 1 },
  { key: "silence_ceiling_dbfs", label: "Silence ceiling", hint: "Never trimmed above this, however loud the track", unit: "dBFS", min: -90, max: 0, step: 1 },
  { key: "blip_max_ms", label: "Ignored blips", hint: "Clicks this short in silence", unit: "ms", min: 0, max: 1000, step: 1 },
  { key: "max_internal_silence_ms", label: "Longest silence inside", hint: "Longer ends the track (hidden tracks)", unit: "s", min: 1, max: 600, step: 1, ms: true },
  { key: "fade_trigger_db", label: "Crossfade trigger", hint: "The crossfade starts after the last moment above this", unit: "dB", min: -60, max: 0, step: 1 },
  { key: "fade_min_ms", label: "Crossfade at least", hint: "Before the cue-out", unit: "s", min: 0, max: 30, step: 0.1, ms: true },
  { key: "fade_max_ms", label: "Crossfade at most", hint: "Before the cue-out", unit: "s", min: 0, max: 60, step: 0.1, ms: true },
];

function AnalysisCard() {
  const isAdmin = useCan("admin");
  const settings = useQuery({
    queryKey: ["settings", "analysis"],
    queryFn: () => unwrap(api.GET("/api/v1/settings/analysis")),
  });
  const save = useAction(
    (body: Schemas["AnalysisSettings"]) => unwrap(api.PUT("/api/v1/settings/analysis", { body })),
    { success: "Saved: every track is analyzed again, in the background", invalidate: [["settings", "analysis"], ["library-status"]] },
  );
  if (settings.isPending) {
    return <Loading />;
  }
  if (settings.isError) {
    return <ErrorView error={settings.error} />;
  }
  return (
    <SettingsCard
      title="Analysis"
      description="Cue points and crossfade points are found when tracks are imported. Changing these analyzes the whole library again (in the background, the station plays on)."
      value={settings.data}
      save={(value) => save.mutate(value)}
      saving={save.isPending}
      canEdit={isAdmin}
    >
      {(draft, set) => (
        <div className="form-grid">
          {ANALYSIS_FIELDS.map((field) => (
            <Field key={field.key} label={field.label} hint={field.hint}>
              <NumberInput
                value={field.ms ? draft[field.key] / 1000 : draft[field.key]}
                min={field.min}
                max={field.max}
                step={field.step}
                suffix={field.unit}
                disabled={!isAdmin}
                onChange={(value) => set({ [field.key]: field.ms ? Math.round(value * 1000) : value })}
              />
            </Field>
          ))}
        </div>
      )}
    </SettingsCard>
  );
}
