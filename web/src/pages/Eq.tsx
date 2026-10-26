// SPDX-License-Identifier: AGPL-3.0-or-later

// The deck EQ: the music EQ (Decks A and B) and the FX EQ, heard as the
// sliders move, and their presets.

import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useMemo, useRef, useState } from "react";
import { type DeckEq, type EqPreset, type EqSettings, api, done, errorMessage, unwrap } from "../api";
import { Card, ConfirmButton, ErrorView, Field, Icon, Loading, Modal, PageHead, useToast } from "../components/ui";
import { t } from "../i18n";
import {
  EQ_KEY,
  FREQUENCIES,
  MAX_GAIN_DB,
  bandLabel,
  latestSender,
  responseDb,
  roundGain,
  useEqSettings,
} from "../lib/eq";
import { useAction, useCan } from "../lib/session";

const PRESETS_KEY = ["eq-presets"];

/** A change of ours, younger than this, is not overwritten by older news. */
const EDITING_MS = 1_500;

type Target = "music" | "fx";

export function Eq() {
  const { data, error } = useEqSettings();
  if (error) {
    return <ErrorView error={error} />;
  }
  if (!data) {
    return <Loading />;
  }
  return <EqEditor saved={data} />;
}

function EqEditor({ saved }: { saved: EqSettings }) {
  const canEdit = useCan("operator");
  const client = useQueryClient();
  const toast = useToast();
  const [draft, setDraft] = useState(saved);
  const lastEdit = useRef(0);
  const sender = useMemo(
    () =>
      latestSender((eq: EqSettings) =>
        unwrap(api.PUT("/api/v1/settings/eq", { body: eq })).then((result) =>
          client.setQueriesData({ queryKey: EQ_KEY }, result),
        ),
      ),
    [client],
  );
  useEffect(() => {
    sender.onError = (error) => toast(errorMessage(error), true);
  }, [sender, toast]);
  // Changes from elsewhere show, unless a slider is moving here.
  useEffect(() => {
    if (!sender.active && Date.now() - lastEdit.current > EDITING_MS) {
      setDraft(saved);
    }
  }, [saved, sender]);

  const change = (target: Target, eq: DeckEq) => {
    lastEdit.current = Date.now();
    const next = { ...draft, [target]: eq };
    setDraft(next);
    sender.push(next);
  };

  return (
    <>
      <PageHead title={t("nav.eq")} description={t("eq.description")} />
      <div className="stack">
        <EqPanel
          title={t("eq.music")}
          hint={t("eq.musicHint")}
          value={draft.music}
          disabled={!canEdit}
          onChange={(eq) => change("music", eq)}
        />
        <EqPanel
          title={t("eq.fx")}
          hint={t("eq.fxHint")}
          value={draft.fx}
          disabled={!canEdit}
          onChange={(eq) => change("fx", eq)}
        />
        <Presets current={draft} canEdit={canEdit} />
        <p className="muted small">{t("eq.headroom")}</p>
      </div>
    </>
  );
}

function usePresets() {
  return useQuery({
    queryKey: PRESETS_KEY,
    queryFn: () => unwrap(api.GET("/api/v1/eq-presets")),
  });
}

function EqPanel({
  title,
  hint,
  value,
  disabled,
  onChange,
}: {
  title: string;
  hint: string;
  value: DeckEq;
  disabled: boolean;
  onChange: (eq: DeckEq) => void;
}) {
  const presets = usePresets();
  const setGain = (band: number, gain: number) =>
    onChange({ ...value, gains_db: value.gains_db.map((g, i) => (i === band ? roundGain(gain) : g)) });
  const loaded = presets.data?.find((preset) => sameGains(preset.gains_db, value.gains_db));

  return (
    <Card
      title={
        <div>
          <h2>{title}</h2>
          <p className="muted small" style={{ margin: 0 }}>
            {hint}
          </p>
        </div>
      }
      actions={
        <>
          <select
            className="input"
            aria-label={t("eq.loadPreset")}
            value={loaded ? String(loaded.id) : ""}
            disabled={disabled || !presets.data}
            onChange={(event) => {
              const preset = presets.data?.find((p) => String(p.id) === event.target.value);
              if (preset) {
                onChange({ ...value, gains_db: preset.gains_db });
              }
            }}
          >
            <option value="">{loaded ? loaded.name : t("eq.loadPreset")}</option>
            {presets.data?.map((preset) => (
              <option key={preset.id} value={preset.id}>
                {preset.name}
              </option>
            ))}
          </select>
          <label className="check">
            <input
              type="checkbox"
              checked={value.enabled}
              disabled={disabled}
              onChange={(event) => onChange({ ...value, enabled: event.target.checked })}
            />
            {t("eq.enabled")}
          </label>
        </>
      }
    >
      <div className={value.enabled ? "eq" : "eq bypassed"}>
        <Curve gains={value.gains_db} enabled={value.enabled} />
        <div className="eq-bands">
          {FREQUENCIES.map((frequency, band) => {
            const gain = value.gains_db[band] ?? 0;
            return (
              <div className="eq-band" key={frequency}>
                <span className="eq-gain">{formatGain(gain)}</span>
                <input
                  type="range"
                  min={-MAX_GAIN_DB}
                  max={MAX_GAIN_DB}
                  step={0.5}
                  value={gain}
                  disabled={disabled}
                  aria-label={t("eq.band", { frequency: bandLabel(frequency) })}
                  aria-valuetext={`${formatGain(gain)} dB`}
                  title={t("eq.resetBand")}
                  onChange={(event) => setGain(band, Number(event.target.value))}
                  onDoubleClick={() => setGain(band, 0)}
                />
                <span className="eq-frequency">{bandLabel(frequency)}</span>
              </div>
            );
          })}
        </div>
        {!value.enabled && <p className="muted small eq-note">{t("eq.bypassedHint")}</p>}
      </div>
    </Card>
  );
}

function formatGain(gain: number): string {
  return gain > 0 ? `+${gain.toFixed(1)}` : gain.toFixed(1);
}

function sameGains(a: number[], b: number[]): boolean {
  return a.length === b.length && a.every((gain, i) => gain === b[i]);
}

// The curve: 20 Hz to 20 kHz on a log scale, ±15 dB: a band's ±12 and
// some of what neighbours add up to.
const WIDTH = 600;
const HEIGHT = 140;
const RANGE_DB = 15;
const x = (frequency: number) => (Math.log10(frequency / 20) / 3) * WIDTH;
const y = (db: number) => HEIGHT / 2 - (Math.max(-RANGE_DB, Math.min(RANGE_DB, db)) / RANGE_DB) * (HEIGHT / 2);

/** The response of the EQ, as the engine's filters have it. */
function Curve({ gains, enabled }: { gains: number[]; enabled: boolean }) {
  const path = useMemo(() => {
    const points: string[] = [];
    for (let step = 0; step <= 150; step++) {
      const frequency = 20 * 1000 ** (step / 150);
      points.push(`${x(frequency).toFixed(1)},${y(responseDb(gains, frequency)).toFixed(1)}`);
    }
    return `M${points.join("L")}`;
  }, [gains]);
  return (
    <svg
      className="eq-curve"
      viewBox={`0 0 ${WIDTH} ${HEIGHT}`}
      preserveAspectRatio="none"
      role="img"
      aria-label={t("eq.curve")}
    >
      {[-12, -6, 6, 12].map((db) => (
        <line key={db} className="eq-grid" x1={0} x2={WIDTH} y1={y(db)} y2={y(db)} />
      ))}
      {FREQUENCIES.map((frequency) => (
        <line key={frequency} className="eq-grid" x1={x(frequency)} x2={x(frequency)} y1={0} y2={HEIGHT} />
      ))}
      <line className="eq-zero" x1={0} x2={WIDTH} y1={y(0)} y2={y(0)} />
      <path className={enabled ? "eq-line" : "eq-line off"} d={path} vectorEffect="non-scaling-stroke" />
    </svg>
  );
}

function Presets({ current, canEdit }: { current: EqSettings; canEdit: boolean }) {
  const presets = usePresets();
  const [naming, setNaming] = useState<{ preset: EqPreset | null; gains: number[] } | null>(null);
  const save = useAction(
    ({ preset, name, gains }: { preset: EqPreset | null; name: string; gains: number[] }) => {
      const body = { name, gains_db: gains };
      return preset
        ? unwrap(api.PUT("/api/v1/eq-presets/{id}", { params: { path: { id: preset.id } }, body }))
        : unwrap(api.POST("/api/v1/eq-presets", { body }));
    },
    { success: t("eq.presetSaved"), invalidate: [PRESETS_KEY], onSuccess: () => setNaming(null) },
  );
  const overwrite = useAction(
    ({ preset, gains }: { preset: EqPreset; gains: number[] }) =>
      unwrap(
        api.PUT("/api/v1/eq-presets/{id}", {
          params: { path: { id: preset.id } },
          body: { name: preset.name, gains_db: gains },
        }),
      ),
    { success: t("eq.presetSaved"), invalidate: [PRESETS_KEY] },
  );
  const remove = useAction(
    (preset: EqPreset) => done(api.DELETE("/api/v1/eq-presets/{id}", { params: { path: { id: preset.id } } })),
    { success: t("eq.presetDeleted"), invalidate: [PRESETS_KEY] },
  );

  return (
    <Card
      title={t("eq.presets")}
      actions={
        canEdit && (
          <>
            <button
              type="button"
              className="btn small"
              onClick={() => setNaming({ preset: null, gains: current.music.gains_db })}
            >
              <Icon name="plus" /> {t("eq.saveMusic")}
            </button>
            <button
              type="button"
              className="btn small"
              onClick={() => setNaming({ preset: null, gains: current.fx.gains_db })}
            >
              <Icon name="plus" /> {t("eq.saveFx")}
            </button>
          </>
        )
      }
      flush
    >
      {presets.isError && <ErrorView error={presets.error} />}
      {presets.isPending && <Loading />}
      <ul className="eq-presets">
        {presets.data?.map((preset) => (
          <li key={preset.id}>
            <span className="ellipsis">{preset.name}</span>
            <MiniCurve gains={preset.gains_db} />
            {canEdit && (
              <span className="row">
                <button
                  type="button"
                  className="btn small ghost"
                  title={t("eq.overwriteMusic")}
                  disabled={overwrite.isPending}
                  onClick={() => overwrite.mutate({ preset, gains: current.music.gains_db })}
                >
                  {t("eq.overwriteMusicShort")}
                </button>
                <button
                  type="button"
                  className="btn small ghost"
                  onClick={() => setNaming({ preset, gains: preset.gains_db })}
                >
                  {t("eq.rename")}
                </button>
                <ConfirmButton
                  className="btn small ghost danger icon"
                  title={t("eq.deletePreset")}
                  question={t("eq.deleteQuestion", { name: preset.name })}
                  confirm={t("eq.deletePreset")}
                  onConfirm={() => remove.mutate(preset)}
                >
                  <Icon name="trash" />
                </ConfirmButton>
              </span>
            )}
          </li>
        ))}
      </ul>
      {naming && (
        <NameModal
          initial={naming.preset?.name ?? ""}
          title={naming.preset ? t("eq.rename") : t("eq.newPreset")}
          busy={save.isPending}
          onClose={() => setNaming(null)}
          onSave={(name) => save.mutate({ preset: naming.preset, name, gains: naming.gains })}
        />
      )}
    </Card>
  );
}

function NameModal({
  initial,
  title,
  busy,
  onClose,
  onSave,
}: {
  initial: string;
  title: string;
  busy: boolean;
  onClose: () => void;
  onSave: (name: string) => void;
}) {
  const [name, setName] = useState(initial);
  const valid = name.trim().length > 0;
  return (
    <Modal
      title={title}
      open
      onClose={onClose}
      footer={
        <>
          <button type="button" className="btn" onClick={onClose}>
            {t("ui.cancel")}
          </button>
          <button
            type="submit"
            form="eq-preset-name"
            className="btn primary"
            disabled={!valid || busy}
          >
            {t("common.save")}
          </button>
        </>
      }
    >
      <form
        id="eq-preset-name"
        onSubmit={(event) => {
          event.preventDefault();
          if (valid) {
            onSave(name.trim());
          }
        }}
      >
        <Field label={t("eq.presetName")}>
          <input
            className="input"
            value={name}
            maxLength={100}
            autoFocus
            onChange={(event) => setName(event.target.value)}
          />
        </Field>
      </form>
    </Modal>
  );
}

/** A preset's curve, small. */
function MiniCurve({ gains }: { gains: number[] }) {
  const path = useMemo(() => {
    const points: string[] = [];
    for (let step = 0; step <= 40; step++) {
      const frequency = 20 * 1000 ** (step / 40);
      points.push(`${x(frequency).toFixed(0)},${y(responseDb(gains, frequency)).toFixed(0)}`);
    }
    return `M${points.join("L")}`;
  }, [gains]);
  return (
    <svg className="eq-mini" viewBox={`0 0 ${WIDTH} ${HEIGHT}`} preserveAspectRatio="none" aria-hidden="true">
      <line className="eq-zero" x1={0} x2={WIDTH} y1={y(0)} y2={y(0)} />
      <path className="eq-line" d={path} vectorEffect="non-scaling-stroke" />
    </svg>
  );
}
