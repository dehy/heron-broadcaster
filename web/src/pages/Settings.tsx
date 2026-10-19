// SPDX-License-Identifier: AGPL-3.0-or-later

// Station settings: autofill, rotation, time zone, programming details,
// analysis and library management.

import { useQuery } from "@tanstack/react-query";
import { type ReactNode, useEffect, useState } from "react";
import { type Schemas, api, unwrap } from "../api";
import { Card, ErrorView, Field, Icon, Loading, NumberInput, PageHead } from "../components/ui";
import { type PlainKey, number, rich, t } from "../i18n";
import { WINDOW_FIELDS } from "../lib/rotation";
import { Link } from "../lib/router";
import { useCategories } from "./Clocks";
import { useAction, useCan } from "../lib/session";

export function Settings() {
  return (
    <>
      <PageHead title={t("nav.settings")} description={t("settings.description")} />
      <QueueCard />
      <RotationCard />
      <StationCard />
      <ProgrammingCard />
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
                {t("track.revert")}
              </button>
            )}
            <button
              type="button"
              className="btn small primary"
              disabled={!dirty || saving}
              onClick={() => save(draft)}
            >
              {t("common.save")}
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
    { success: t("categories.saved"), invalidate: [["settings", "queue"]] },
  );
  if (settings.isPending) {
    return <Loading />;
  }
  if (settings.isError) {
    return <ErrorView error={settings.error} />;
  }
  return (
    <SettingsCard
      title={t("nav.queue")}
      description={rich("settings.queueHelp", {
        schedule: <Link to="/schedule">{t("clocks.scheduleLink")}</Link>,
        clocks: <Link to="/clocks">{t("settings.clocksLink")}</Link>,
        log: <Link to="/rotation">{t("settings.logLink")}</Link>,
      })}
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
            {t("source.rotation")}
          </label>
          <Field label={t("settings.minQueued")} hint={t("settings.minQueuedHint")}>
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

const WINDOW_LABELS: Record<string, PlainKey> = {
  album_min: "separation.album",
  artist_min: "separation.artist",
  title_min: "separation.title",
  track_min: "separation.track",
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
    { success: t("categories.saved"), invalidate: [["settings", "rotation"]] },
  );
  if (settings.isPending) {
    return <Loading />;
  }
  if (settings.isError) {
    return <ErrorView error={settings.error} />;
  }
  return (
    <SettingsCard
      title={t("settings.rotation")}
      description={t("settings.rotationHelp")}
      value={settings.data}
      save={(value) => save.mutate(value)}
      saving={save.isPending}
      canEdit={canEdit}
    >
      {(draft, set) => (
        <>
          <div className="form-grid">
            {WINDOW_FIELDS.map(([key]) => (
              <Field key={key} label={t(WINDOW_LABELS[key] ?? "separation.track")}>
                <NumberInput
                  value={draft.separation[key]}
                  min={0}
                  max={10_080}
                  step={1}
                  suffix={t("common.min")}
                  disabled={!canEdit}
                  onChange={(minutes) => set({ separation: { ...draft.separation, [key]: Math.round(minutes) } })}
                />
              </Field>
            ))}
          </div>
          <div className="form-grid">
            <Field label={t("settings.fallback")} hint={t("settings.fallbackHint")}>
              <select
                className="input"
                value={draft.fallback_category_id ?? ""}
                disabled={!canEdit}
                onChange={(e) => set({ fallback_category_id: e.target.value ? Number(e.target.value) : null })}
              >
                <option value="">{t("settings.wholeLibrary")}</option>
                {categories.data?.map((category) => (
                  <option key={category.id} value={category.id}>
                    {category.name}
                  </option>
                ))}
              </select>
            </Field>
            <Field label={t("settings.playFactor")} hint={t("settings.playFactorHint")}>
              <NumberInput
                value={draft.weights.play_factor}
                min={0.5}
                max={1}
                step={0.05}
                disabled={!canEdit}
                onChange={(play_factor) => set({ weights: { ...draft.weights, play_factor } })}
              />
            </Field>
            <Field label={t("settings.requestFactor")} hint={t("settings.requestFactorHint")}>
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
    { success: t("categories.saved"), invalidate: [["settings", "station"]] },
  );
  if (settings.isPending) {
    return <Loading />;
  }
  if (settings.isError) {
    return <ErrorView error={settings.error} />;
  }
  return (
    <SettingsCard
      title={t("nav.station")}
      description={t("settings.stationHelp")}
      value={settings.data}
      save={(value) => save.mutate(value)}
      saving={save.isPending}
      canEdit={isAdmin}
    >
      {(draft, set) => (
        <Field label={t("settings.timeZone")} hint={t("settings.timeZoneHint")}>
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

/** A scale typed as `soft, medium, hard`. */
function ScaleInput({
  value,
  onChange,
  disabled,
}: {
  value: string[];
  onChange: (value: string[]) => void;
  disabled: boolean;
}) {
  const [text, setText] = useState(value.join(", "));
  useEffect(() => {
    setText((current) => (split(current).join(",") === value.join(",") ? current : value.join(", ")));
  }, [value]);
  return (
    <input
      className="input"
      value={text}
      disabled={disabled}
      onChange={(event) => {
        setText(event.target.value);
        onChange(split(event.target.value));
      }}
    />
  );
}

function split(text: string): string[] {
  return text
    .split(",")
    .map((value) => value.trim())
    .filter(Boolean);
}

function ProgrammingCard() {
  const canEdit = useCan("operator");
  const settings = useQuery({
    queryKey: ["settings", "programming"],
    queryFn: () => unwrap(api.GET("/api/v1/settings/programming")),
  });
  const save = useAction(
    (body: Schemas["ProgrammingSettings"]) => unwrap(api.PUT("/api/v1/settings/programming", { body })),
    { success: t("categories.saved"), invalidate: [["settings", "programming"], ["tracks"], ["track"]] },
  );
  const details = useAction(
    (all: boolean) => unwrap(api.POST("/api/v1/library/musicbrainz-details", { body: { all } })),
    { invalidate: [["library-tasks"]] },
  );
  const tasks = useQuery({
    queryKey: ["library-tasks"],
    queryFn: () => unwrap(api.GET("/api/v1/library/tasks", { params: { query: { limit: 50 } } })),
    refetchInterval: (query) =>
      query.state.data?.some(
        (task) =>
          task.request.kind === "musicbrainz_details" && (task.status === "pending" || task.status === "running"),
      )
        ? 2000
        : false,
  });
  const lookup = tasks.data?.find((task) => task.request.kind === "musicbrainz_details");
  if (settings.isPending) {
    return <Loading />;
  }
  if (settings.isError) {
    return <ErrorView error={settings.error} />;
  }
  return (
    <>
      <SettingsCard
        title={t("settings.programming")}
        description={t("settings.programmingHelp")}
        value={settings.data}
        save={(value) => save.mutate(value)}
        saving={save.isPending}
        canEdit={canEdit}
      >
        {(draft, set) => (
          <>
            <div className="form-grid">
              <Field label={t("settings.minPlay")} hint={t("settings.minPlayHint")}>
                <NumberInput
                  value={draft.min_play_s}
                  min={0}
                  max={3600}
                  step={1}
                  suffix="s"
                  disabled={!canEdit}
                  onChange={(min_play_s) => set({ min_play_s })}
                />
              </Field>
              <Field label={t("settings.moods")} hint={t("settings.scaleHint")}>
                <ScaleInput value={draft.moods} disabled={!canEdit} onChange={(moods) => set({ moods })} />
              </Field>
              <Field label={t("settings.soundCodes")} hint={t("settings.scaleHint")}>
                <ScaleInput
                  value={draft.sound_codes}
                  disabled={!canEdit}
                  onChange={(sound_codes) => set({ sound_codes })}
                />
              </Field>
            </div>
            <fieldset className="stack">
              <legend>{t("settings.eras")}</legend>
              {draft.eras.length === 0 && (
                <p className="muted small" style={{ margin: 0 }}>
                  {t("settings.erasHelp")}
                </p>
              )}
              {draft.eras.map((era, index) => {
                const change = (changes: Partial<Schemas["Era"]>) =>
                  set({ eras: draft.eras.map((e, i) => (i === index ? { ...e, ...changes } : e)) });
                const year = (value: string) => (value.trim() === "" ? null : Number(value));
                return (
                  <div key={index} className="row" style={{ flexWrap: "nowrap" }}>
                    <input
                      className="input"
                      aria-label={t("settings.era")}
                      value={era.name}
                      disabled={!canEdit}
                      onChange={(event) => change({ name: event.target.value })}
                    />
                    <input
                      className="input num"
                      type="number"
                      aria-label={t("settings.eraFrom")}
                      placeholder={t("settings.eraFromPlaceholder")}
                      value={era.from_year ?? ""}
                      disabled={!canEdit}
                      onChange={(event) => change({ from_year: year(event.target.value) })}
                    />
                    <input
                      className="input num"
                      type="number"
                      aria-label={t("settings.eraTo")}
                      placeholder={t("settings.eraToPlaceholder")}
                      value={era.to_year ?? ""}
                      disabled={!canEdit}
                      onChange={(event) => change({ to_year: year(event.target.value) })}
                    />
                    {canEdit && (
                      <button
                        type="button"
                        className="btn ghost small icon danger"
                        aria-label={t("settings.removeEra", { name: era.name })}
                        onClick={() => set({ eras: draft.eras.filter((_, i) => i !== index) })}
                      >
                        <Icon name="close" />
                      </button>
                    )}
                  </div>
                );
              })}
              {canEdit && (
                <div>
                  <button
                    type="button"
                    className="btn small"
                    onClick={() => set({ eras: [...draft.eras, { name: "", from_year: null, to_year: null }] })}
                  >
                    <Icon name="plus" size={14} /> {t("settings.addEra")}
                  </button>
                </div>
              )}
            </fieldset>
          </>
        )}
      </SettingsCard>
      <Card title={t("settings.musicbrainz")}>
        <div className="stack">
          <p className="muted small" style={{ margin: 0 }}>
            {t("settings.musicbrainzHelp")}
          </p>
          {canEdit && (
            <div className="row">
              <button type="button" className="btn" disabled={details.isPending} onClick={() => details.mutate(false)}>
                {t("settings.lookUpNew")}
              </button>
              <button
                type="button"
                className="btn ghost"
                disabled={details.isPending}
                onClick={() => details.mutate(true)}
              >
                {t("settings.lookUpAll")}
              </button>
            </div>
          )}
          {lookup && (
            <p className="small" style={{ margin: 0 }}>
              {lookup.status === "pending" && t("settings.lookupPending")}
              {lookup.status === "running" && t("settings.lookupRunning")}
              {lookup.status === "failed" && (
                <span className="bad">{t("selection.failed", { error: lookup.error ?? "" })}</span>
              )}
              {lookup.status === "done" &&
                lookup.details &&
                t("settings.lookupDone", {
                  releases: number(lookup.details.releases),
                  tracks: number(lookup.details.tracks),
                })}
              {lookup.status === "done" &&
                (lookup.details?.failed ?? 0) > 0 &&
                t("settings.lookupFailed", { count: lookup.details?.failed ?? 0 })}
            </p>
          )}
        </div>
      </Card>
    </>
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
    { success: t("categories.saved"), invalidate: [["settings", "library"]], onSuccess: () => setKey("") },
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
        title={t("nav.library")}
        description={rich("settings.libraryHelp", {
          fields: <code>{"{album_artist} {artist} {album} {title} {year} {track:02} {disc} {disc-} {ext}"}</code>,
        })}
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
            <Field label={t("settings.layout")} hint={t("settings.layoutHint")}>
              <input
                className="input mono"
                value={draft.layout}
                disabled={!isAdmin}
                onChange={(e) => set({ layout: e.target.value })}
              />
            </Field>
            <div className="form-grid">
              <Field label={t("settings.minScore")} hint={t("settings.minScoreHint")}>
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
                {t("settings.saveCover")}
              </label>
              <label className="check">
                <input
                  type="checkbox"
                  checked={draft.embed_cover}
                  disabled={!isAdmin}
                  onChange={(e) => set({ embed_cover: e.target.checked })}
                />
                {t("settings.embedCover")}
              </label>
            </div>
          </>
        )}
      </SettingsCard>
      <Card title={t("track.identification")}>
        <div className="stack">
          <p className="muted small" style={{ margin: 0 }}>
            {t("settings.acoustidHelp")}
          </p>
          <p style={{ margin: 0 }}>
            {current.acoustid_key_set ? (
              <span className="good">{t("settings.keySet")}</span>
            ) : (
              <span className="warn">{t("settings.noKey")}</span>
            )}
          </p>
          {isAdmin && (
            <div className="row">
              <label style={{ flex: "1 1 16rem" }}>
                <span className="visually-hidden">{t("settings.key")}</span>
                <input
                  className="input mono"
                  type="password"
                  autoComplete="off"
                  placeholder={current.acoustid_key_set ? t("settings.replaceKey") : t("settings.keyPlaceholder")}
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
                {t("settings.saveKey")}
              </button>
              {current.acoustid_key_set && (
                <button type="button" className="btn danger" onClick={() => save.mutate({ acoustid_key: null })}>
                  {t("settings.removeKey")}
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
  label: PlainKey;
  hint: PlainKey;
  unit: string;
  min: number;
  max: number;
  step: number;
  ms?: boolean;
}[] = [
  { key: "start_threshold_db", label: "settings.startThreshold", hint: "settings.startThresholdHint", unit: "dB", min: -80, max: 0, step: 1 },
  { key: "end_threshold_db", label: "settings.endThreshold", hint: "settings.endThresholdHint", unit: "dB", min: -80, max: 0, step: 1 },
  { key: "silence_ceiling_dbfs", label: "settings.silenceCeiling", hint: "settings.silenceCeilingHint", unit: "dBFS", min: -90, max: 0, step: 1 },
  { key: "blip_max_ms", label: "settings.blips", hint: "settings.blipsHint", unit: "ms", min: 0, max: 1000, step: 1 },
  { key: "max_internal_silence_ms", label: "settings.internalSilence", hint: "settings.internalSilenceHint", unit: "s", min: 1, max: 600, step: 1, ms: true },
  { key: "fade_trigger_db", label: "settings.fadeTrigger", hint: "settings.fadeTriggerHint", unit: "dB", min: -60, max: 0, step: 1 },
  { key: "fade_min_ms", label: "settings.fadeMin", hint: "settings.fadeMinHint", unit: "s", min: 0, max: 30, step: 0.1, ms: true },
  { key: "fade_max_ms", label: "settings.fadeMax", hint: "settings.fadeMaxHint", unit: "s", min: 0, max: 60, step: 0.1, ms: true },
];

function AnalysisCard() {
  const isAdmin = useCan("admin");
  const settings = useQuery({
    queryKey: ["settings", "analysis"],
    queryFn: () => unwrap(api.GET("/api/v1/settings/analysis")),
  });
  const save = useAction(
    (body: Schemas["AnalysisSettings"]) => unwrap(api.PUT("/api/v1/settings/analysis", { body })),
    { success: t("settings.analysisSaved"), invalidate: [["settings", "analysis"], ["library-status"]] },
  );
  if (settings.isPending) {
    return <Loading />;
  }
  if (settings.isError) {
    return <ErrorView error={settings.error} />;
  }
  return (
    <SettingsCard
      title={t("settings.analysis")}
      description={t("settings.analysisHelp")}
      value={settings.data}
      save={(value) => save.mutate(value)}
      saving={save.isPending}
      canEdit={isAdmin}
    >
      {(draft, set) => (
        <div className="form-grid">
          {ANALYSIS_FIELDS.map((field) => (
            <Field key={field.key} label={t(field.label)} hint={t(field.hint)}>
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
