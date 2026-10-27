// SPDX-License-Identifier: AGPL-3.0-or-later

// Medleys: song hooks and liners picked from a recipe, checked and
// rearranged here, then mixed into a library file by heron-librarian.
// See docs/medleys.md.

import { useQuery } from "@tanstack/react-query";
import { type FormEvent, useEffect, useState } from "react";
import { type Schemas, api, done, unwrap } from "../api";
import { useLabels } from "../components/SelectionActions";
import { TaskState, useTask } from "../components/Task";
import { Card, ConfirmButton, Empty, ErrorView, Field, Icon, Loading, PageHead } from "../components/ui";
import { number, t } from "../i18n";
import { ago, duration, trackName } from "../lib/format";
import { plannedLength } from "../lib/medley";
import { usePreview } from "../lib/preview";
import { Link, navigate } from "../lib/router";
import { useAction, useCan } from "../lib/session";

type Medley = Schemas["Medley"];
type Recipe = Schemas["Recipe"];
type PartView = Schemas["PartView"];

const DEFAULT_RECIPE: Recipe = {
  category_ids: [],
  label_id: null,
  hooks: 6,
  order: "picked",
  hook_min_ms: 4_000,
  hook_max_ms: 8_000,
  crossfade_ms: 500,
  liner_category_id: null,
  liner_every: 1,
  liner_at_start: false,
  liner_at_end: false,
  liner_mode: "between",
  duck_db: -10,
};

function useMedleys() {
  return useQuery({
    queryKey: ["medleys"],
    queryFn: () => unwrap(api.GET("/api/v1/medleys")),
    // While a render runs, or its file waits for the library scan.
    refetchInterval: (query) =>
      query.state.data?.some(
        (medley) =>
          medley.task_status === "pending" ||
          medley.task_status === "running" ||
          (medley.path != null && medley.track_id == null),
      )
        ? 3_000
        : false,
  });
}

function useCategories() {
  return useQuery({ queryKey: ["categories"], queryFn: () => unwrap(api.GET("/api/v1/categories")) });
}

export function Medleys() {
  const canEdit = useCan("operator");
  const medleys = useMedleys();
  return (
    <>
      <PageHead
        title={t("nav.medleys")}
        description={t("medleys.description")}
        actions={
          canEdit && (
            <button type="button" className="btn primary" onClick={() => navigate("/medleys/new")}>
              <Icon name="plus" /> {t("medleys.new")}
            </button>
          )
        }
      />
      <Card flush>
        {medleys.isPending && <Loading />}
        {medleys.isError && <ErrorView error={medleys.error} />}
        {medleys.data?.length === 0 && <Empty>{t("medleys.none")}</Empty>}
        {medleys.data && medleys.data.length > 0 && (
          <table className="table">
            <thead>
              <tr>
                <th>{t("medleys.name")}</th>
                <th>{t("medleys.category")}</th>
                <th>{t("medleys.hooks")}</th>
                <th>{t("medleys.rendered")}</th>
              </tr>
            </thead>
            <tbody>
              {medleys.data.map((medley) => (
                <tr key={medley.id}>
                  <td>
                    <Link to={`/medleys/${medley.id}`}>{medley.name}</Link>
                  </td>
                  <td>{medley.category_name ?? <span className="muted">–</span>}</td>
                  <td className="num">{number(medley.parts.filter((part) => part.kind === "hook").length)}</td>
                  <td className="small muted">
                    {medley.task_status === "failed"
                      ? t("medleys.renderFailed")
                      : medley.rendered_at_unix_ms
                        ? ago(medley.rendered_at_unix_ms)
                        : t("medleys.notRendered")}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </Card>
    </>
  );
}

/** One medley: its recipe, its current version, and building a new one. */
export function MedleyEditor({ id }: { id: number | null }) {
  const canEdit = useCan("operator");
  const medleys = useMedleys();
  const medley = id === null ? null : medleys.data?.find((m) => m.id === id);
  if (id !== null && medleys.isPending) {
    return <Loading />;
  }
  if (id !== null && !medley) {
    return <ErrorView error={medleys.error ?? new Error(t("medleys.notFound"))} />;
  }
  return (
    <>
      <PageHead
        title={medley?.name ?? t("medleys.new")}
        description={
          <Link to="/medleys">
            ← {t("nav.medleys")}
          </Link>
        }
      />
      <RecipeCard key={medley?.id ?? "new"} medley={medley ?? null} canEdit={canEdit} />
      {medley && <BuildCard key={`build-${medley.id}`} medley={medley} canEdit={canEdit} />}
    </>
  );
}

function RecipeCard({ medley, canEdit }: { medley: Medley | null; canEdit: boolean }) {
  const categories = useCategories();
  const labels = useLabels();
  const [name, setName] = useState(medley?.name ?? "");
  const [categoryId, setCategoryId] = useState<number | null>(medley?.category_id ?? null);
  const [recipe, setRecipe] = useState<Recipe>(medley?.recipe ?? DEFAULT_RECIPE);
  const set = (changes: Partial<Recipe>) => setRecipe((r) => ({ ...r, ...changes }));
  const save = useAction(
    () => {
      const body = { name: name.trim(), category_id: categoryId, recipe };
      return medley
        ? unwrap(api.PUT("/api/v1/medleys/{id}", { params: { path: { id: medley.id } }, body }))
        : unwrap(api.POST("/api/v1/medleys", { body }));
    },
    {
      success: t("medleys.saved"),
      invalidate: [["medleys"], ["tracks"], ["track"]],
      onSuccess: (saved) => {
        if (!medley) {
          navigate(`/medleys/${saved.id}`, { replace: true });
        }
      },
    },
  );
  const remove = useAction(
    () => done(api.DELETE("/api/v1/medleys/{id}", { params: { path: { id: medley?.id ?? 0 } } })),
    { success: t("medleys.deleted"), invalidate: [["medleys"]], onSuccess: () => navigate("/medleys") },
  );
  const disabled = !canEdit;
  const seconds = (ms: number) => String(ms / 1000);
  const fromSeconds = (text: string) => Math.round(Number(text.replace(",", ".")) * 1000);
  const submit = (event: FormEvent) => {
    event.preventDefault();
    save.mutate(undefined);
  };
  return (
    <Card
      title={t("medleys.recipe")}
      actions={
        canEdit &&
        medley && (
          <ConfirmButton
            className="btn small danger"
            question={t("medleys.deleteQuestion", { name: medley.name })}
            confirm={t("medleys.delete")}
            onConfirm={() => remove.mutate(undefined)}
          >
            <Icon name="trash" size={14} /> {t("medleys.delete")}
          </ConfirmButton>
        )
      }
    >
      <form className="stack" onSubmit={submit}>
        <div className="form-grid">
          <Field label={t("medleys.name")}>
            <input
              className="input"
              value={name}
              maxLength={255}
              required
              disabled={disabled}
              onChange={(event) => setName(event.target.value)}
            />
          </Field>
          <Field label={t("medleys.category")} hint={t("medleys.categoryHelp")}>
            <select
              className="input"
              value={categoryId ?? ""}
              disabled={disabled}
              onChange={(event) => setCategoryId(event.target.value ? Number(event.target.value) : null)}
            >
              <option value="">{t("medleys.noCategory")}</option>
              {categories.data?.map((category) => (
                <option key={category.id} value={category.id}>
                  {category.name}
                </option>
              ))}
            </select>
          </Field>
        </div>

        <fieldset>
          <legend>{t("medleys.songs")}</legend>
          <div className="stack">
            <div className="chips" role="group" aria-label={t("medleys.songCategories")}>
              {categories.data?.map((category) => (
                <label key={category.id} className="row small" style={{ gap: "0.3rem" }}>
                  <input
                    type="checkbox"
                    disabled={disabled}
                    checked={recipe.category_ids.includes(category.id)}
                    onChange={(event) =>
                      set({
                        category_ids: event.target.checked
                          ? [...recipe.category_ids, category.id]
                          : recipe.category_ids.filter((id) => id !== category.id),
                      })
                    }
                  />
                  {category.name}
                </label>
              ))}
            </div>
            <div className="form-grid">
              <Field label={t("medleys.label")}>
                <select
                  className="input"
                  value={recipe.label_id ?? ""}
                  disabled={disabled}
                  onChange={(event) => set({ label_id: event.target.value ? Number(event.target.value) : null })}
                >
                  <option value="">{t("medleys.anyLabel")}</option>
                  {labels.data?.map((label) => (
                    <option key={label.id} value={label.id}>
                      {label.name}
                    </option>
                  ))}
                </select>
              </Field>
              <Field label={t("medleys.order")}>
                <select
                  className="input"
                  value={recipe.order}
                  disabled={disabled}
                  onChange={(event) => set({ order: event.target.value as Recipe["order"] })}
                >
                  <option value="picked">{t("medleys.orderPicked")}</option>
                  <option value="year">{t("medleys.orderYear")}</option>
                  <option value="bpm">{t("medleys.orderBpm")}</option>
                </select>
              </Field>
            </div>
          </div>
        </fieldset>

        <fieldset>
          <legend>{t("medleys.hooks")}</legend>
          <div className="form-grid">
            <Field label={t("medleys.hookCount")}>
              <input
                className="input num"
                type="number"
                min={2}
                max={30}
                value={recipe.hooks}
                disabled={disabled}
                onChange={(event) => set({ hooks: Number(event.target.value) })}
              />
            </Field>
            <Field label={t("medleys.crossfade")}>
              <input
                className="input num"
                type="number"
                min={0}
                max={5}
                step={0.1}
                value={seconds(recipe.crossfade_ms)}
                disabled={disabled}
                onChange={(event) => set({ crossfade_ms: fromSeconds(event.target.value) })}
              />
            </Field>
            <Field label={t("medleys.hookMin")} hint={t("medleys.hookMinHelp")}>
              <input
                className="input num"
                type="number"
                min={1}
                max={60}
                step={0.5}
                value={seconds(recipe.hook_min_ms)}
                disabled={disabled}
                onChange={(event) => set({ hook_min_ms: fromSeconds(event.target.value) })}
              />
            </Field>
            <Field label={t("medleys.hookMax")} hint={t("medleys.hookMaxHelp")}>
              <input
                className="input num"
                type="number"
                min={1}
                max={60}
                step={0.5}
                value={seconds(recipe.hook_max_ms)}
                disabled={disabled}
                onChange={(event) => set({ hook_max_ms: fromSeconds(event.target.value) })}
              />
            </Field>
          </div>
        </fieldset>

        <fieldset>
          <legend>{t("medleys.liners")}</legend>
          <div className="stack">
            <div className="form-grid">
              <Field label={t("medleys.linerCategory")}>
                <select
                  className="input"
                  value={recipe.liner_category_id ?? ""}
                  disabled={disabled}
                  onChange={(event) =>
                    set({ liner_category_id: event.target.value ? Number(event.target.value) : null })
                  }
                >
                  <option value="">{t("medleys.noLiners")}</option>
                  {categories.data?.map((category) => (
                    <option key={category.id} value={category.id}>
                      {category.name}
                    </option>
                  ))}
                </select>
              </Field>
              <Field label={t("medleys.linerEvery")}>
                <input
                  className="input num"
                  type="number"
                  min={1}
                  max={30}
                  value={recipe.liner_every}
                  disabled={disabled || recipe.liner_category_id === null}
                  onChange={(event) => set({ liner_every: Number(event.target.value) })}
                />
              </Field>
              <Field label={t("medleys.linerMode")}>
                <select
                  className="input"
                  value={recipe.liner_mode}
                  disabled={disabled || recipe.liner_category_id === null}
                  onChange={(event) => set({ liner_mode: event.target.value as Recipe["liner_mode"] })}
                >
                  <option value="between">{t("medleys.linerBetween")}</option>
                  <option value="over">{t("medleys.linerOver")}</option>
                </select>
              </Field>
              {recipe.liner_mode === "over" && (
                <Field label={t("medleys.duck")}>
                  <input
                    className="input num"
                    type="number"
                    min={-40}
                    max={0}
                    step={1}
                    value={recipe.duck_db}
                    disabled={disabled || recipe.liner_category_id === null}
                    onChange={(event) => set({ duck_db: Number(event.target.value) })}
                  />
                </Field>
              )}
            </div>
            <div className="row">
              <label className="row small" style={{ gap: "0.3rem" }}>
                <input
                  type="checkbox"
                  checked={recipe.liner_at_start}
                  disabled={disabled || recipe.liner_category_id === null}
                  onChange={(event) => set({ liner_at_start: event.target.checked })}
                />
                {t("medleys.linerAtStart")}
              </label>
              <label className="row small" style={{ gap: "0.3rem" }}>
                <input
                  type="checkbox"
                  checked={recipe.liner_at_end}
                  disabled={disabled || recipe.liner_category_id === null}
                  onChange={(event) => set({ liner_at_end: event.target.checked })}
                />
                {t("medleys.linerAtEnd")}
              </label>
            </div>
          </div>
        </fieldset>

        {canEdit && (
          <div className="row">
            <button
              type="submit"
              className="btn primary"
              disabled={save.isPending || !name.trim() || recipe.category_ids.length === 0}
            >
              {t("common.save")}
            </button>
            {recipe.category_ids.length === 0 && <span className="small muted">{t("medleys.pickCategory")}</span>}
          </div>
        )}
      </form>
    </Card>
  );
}

function BuildCard({ medley, canEdit }: { medley: Medley; canEdit: boolean }) {
  const preview = usePreview();
  // A new version being built; the current one otherwise.
  const [plan, setPlan] = useState<PartView[] | null>(null);
  const [qualified, setQualified] = useState<number | null>(null);
  const [taskId, setTaskId] = useState<number | null>(medley.task_id ?? null);
  const task = useTask(taskId);
  const build = useAction(
    () => unwrap(api.POST("/api/v1/medleys/{id}/plan", { params: { path: { id: medley.id } } })),
    {
      onSuccess: (result) => {
        setPlan(result.parts);
        setQualified(result.qualified);
      },
    },
  );
  const replace = useAction(
    async (index: number) => {
      const parts = plan ?? [];
      const part = parts[index];
      if (!part) {
        return null;
      }
      const other = await unwrap(
        api.POST("/api/v1/medleys/{id}/pick", {
          params: { path: { id: medley.id } },
          body: { kind: part.kind, exclude: parts.map((p) => p.track_id) },
        }),
      );
      return { index, other };
    },
    {
      onSuccess: (result) => {
        if (result) {
          setPlan((parts) => (parts ?? []).map((part, i) => (i === result.index ? result.other : part)));
        }
      },
    },
  );
  const render = useAction(
    () =>
      unwrap(
        api.POST("/api/v1/medleys/{id}/render", {
          params: { path: { id: medley.id } },
          body: {
            parts: (plan ?? []).map(({ kind, track_id, from_ms, to_ms }) => ({ kind, track_id, from_ms, to_ms })),
          },
        }),
      ),
    {
      success: t("medleys.renderAsked"),
      invalidate: [["medleys"]],
      onSuccess: (asked) => {
        setTaskId(asked.id);
        setPlan(null);
      },
    },
  );
  // The songs and liners to choose from, while a new version is edited.
  const hookChoices = useCandidates(medley.id, "hook", canEdit && plan !== null);
  const linerChoices = useCandidates(
    medley.id,
    "liner",
    canEdit && plan !== null && medley.recipe.liner_category_id != null,
  );
  const choices = (kind: string) => (kind === "hook" ? hookChoices.data : linerChoices.data) ?? [];
  const inPlan = new Set((plan ?? []).map((part) => part.track_id));
  const setPart = (index: number, chosen: PartView) =>
    setPlan((current) => (current ?? []).map((part, i) => (i === index ? chosen : part)));
  const addPart = (chosen: PartView) => setPlan((current) => [...(current ?? []), chosen]);

  const status = task.data?.status;
  // The version and its track change once rendered, and again once scanned.
  const { refetch } = useMedleys();
  useEffect(() => {
    if (status === "done" || status === "failed") {
      void refetch();
    }
  }, [status, refetch]);

  const parts = plan ?? medley.parts;
  const move = (index: number, by: number) =>
    setPlan((current) => {
      const next = [...(current ?? [])];
      const [part] = next.splice(index, 1);
      if (part) {
        next.splice(index + by, 0, part);
      }
      return next;
    });
  const hooks = parts.filter((part) => part.kind === "hook").length;

  return (
    <Card
      title={plan ? t("medleys.newVersion") : t("medleys.currentVersion")}
      actions={
        canEdit && (
          <>
            {plan && (
              <button type="button" className="btn small" onClick={() => setPlan(null)}>
                {t("track.revert")}
              </button>
            )}
            {!plan && medley.parts.length > 0 && (
              <button
                type="button"
                className="btn small"
                title={t("medleys.editHelp")}
                onClick={() => {
                  setPlan(medley.parts);
                  setQualified(null);
                }}
              >
                <Icon name="edit" size={14} /> {t("medleys.edit")}
              </button>
            )}
            <button type="button" className="btn small" disabled={build.isPending} onClick={() => build.mutate(undefined)}>
              {plan ? t("medleys.buildAgain") : t("medleys.build")}
            </button>
            {plan && (
              <button
                type="button"
                className="btn small primary"
                disabled={render.isPending || hooks === 0}
                onClick={() => render.mutate(undefined)}
              >
                {t("medleys.render")}
              </button>
            )}
          </>
        )
      }
      flush
    >
      <div className="card-head stack" style={{ gap: "0.3rem" }}>
        <span className="small muted">
          {t("medleys.summary", {
            hooks: number(hooks),
            liners: number(parts.length - hooks),
            length: duration(plannedLength(parts, medley.recipe)),
          })}
          {plan && qualified !== null && qualified < medley.recipe.hooks && (
            <> · {t("medleys.fewSongs", { qualified: number(qualified), wanted: number(medley.recipe.hooks) })}</>
          )}
        </span>
        {!plan && <TaskState task={task.data} />}
        {!plan && medley.track_id != null && (
          <span className="row small">
            <button
              type="button"
              className="btn small"
              aria-pressed={preview.playing === medley.track_id}
              onClick={() => preview.toggle({ id: medley.track_id ?? 0 })}
            >
              <Icon name={preview.playing === medley.track_id ? "stop" : "play"} size={14} />{" "}
              {preview.playing === medley.track_id ? t("medleys.stop") : t("medleys.listenMedley")}
            </button>
            <Link to={`/library/${medley.track_id}`}>{t("medleys.openTrack")}</Link>
          </span>
        )}
        {!plan && medley.path && medley.track_id == null && status === "done" && (
          <span className="small muted">{t("medleys.waitingScan")}</span>
        )}
      </div>
      {parts.length === 0 ? (
        <Empty>{canEdit ? t("medleys.buildHelp") : t("medleys.notBuilt")}</Empty>
      ) : (
        <ol className="list">
          {parts.map((part, index) => {
            const playing = preview.playing === part.track_id;
            return (
              <li key={`${index}-${part.track_id}`} className="queue-item">
                <span className="index">{index + 1}</span>
                <div className="meta">
                  <div className="ellipsis">
                    <Link to={`/library/${part.track_id}`}>{trackName(part)}</Link>
                  </div>
                  <div className="row small muted" style={{ gap: "0.4rem" }}>
                    <span className={part.kind === "hook" ? "badge accent" : "badge"}>
                      {part.kind === "hook" ? t("medleys.hook") : t("medleys.liner")}
                    </span>
                    <span className="num">
                      {duration(part.from_ms, true)} – {duration(part.to_ms, true)} ·{" "}
                      {duration(part.to_ms - part.from_ms, true)}
                    </span>
                  </div>
                </div>
                <div className="row" style={{ flexWrap: "nowrap", gap: "0.25rem" }}>
                  <button
                    type="button"
                    className="btn ghost small icon"
                    aria-label={t(playing ? "library.stopPreviewNamed" : "library.previewNamed", { name: trackName(part) })}
                    aria-pressed={playing}
                    onClick={() => preview.toggle({ id: part.track_id, cue_in_ms: part.from_ms, cue_out_ms: part.to_ms })}
                  >
                    <Icon name={playing ? "stop" : "play"} />
                  </button>
                  {canEdit && plan && (
                    <>
                      <ChoiceSelect
                        label={t("medleys.chooseFor", { name: trackName(part) })}
                        placeholder={part.kind === "hook" ? t("medleys.chooseSong") : t("medleys.chooseLiner")}
                        // A song once per medley; a liner anywhere but twice in a row.
                        choices={choices(part.kind).filter((choice) =>
                          part.kind === "hook" ? !inPlan.has(choice.track_id) : choice.track_id !== part.track_id,
                        )}
                        onChoose={(chosen) => setPart(index, chosen)}
                      />
                      <button
                        type="button"
                        className="btn ghost small icon"
                        title={t("medleys.replace")}
                        aria-label={t("medleys.replaceNamed", { name: trackName(part) })}
                        disabled={replace.isPending}
                        onClick={() => replace.mutate(index)}
                      >
                        <Icon name="undo" />
                      </button>
                      <button
                        type="button"
                        className="btn ghost small icon"
                        aria-label={t("medleys.moveUp", { name: trackName(part) })}
                        disabled={index === 0}
                        onClick={() => move(index, -1)}
                      >
                        ↑
                      </button>
                      <button
                        type="button"
                        className="btn ghost small icon"
                        aria-label={t("medleys.moveDown", { name: trackName(part) })}
                        disabled={index === parts.length - 1}
                        onClick={() => move(index, 1)}
                      >
                        ↓
                      </button>
                      <button
                        type="button"
                        className="btn ghost small icon danger"
                        aria-label={t("medleys.removeNamed", { name: trackName(part) })}
                        onClick={() => setPlan((current) => (current ?? []).filter((_, i) => i !== index))}
                      >
                        <Icon name="close" />
                      </button>
                    </>
                  )}
                </div>
              </li>
            );
          })}
        </ol>
      )}
      {canEdit && plan && (
        <div className="card-head row" style={{ justifyContent: "flex-start" }}>
          <ChoiceSelect
            label={t("medleys.addHook")}
            placeholder={t("medleys.addHook")}
            choices={hookChoices.data?.filter((choice) => !inPlan.has(choice.track_id)) ?? []}
            onChoose={addPart}
          />
          {medley.recipe.liner_category_id != null && (
            <ChoiceSelect
              label={t("medleys.addLiner")}
              placeholder={t("medleys.addLiner")}
              choices={linerChoices.data ?? []}
              onChoose={addPart}
            />
          )}
        </div>
      )}
    </Card>
  );
}

function useCandidates(id: number, kind: "hook" | "liner", enabled: boolean) {
  return useQuery({
    queryKey: ["medley-candidates", id, kind],
    enabled,
    queryFn: () =>
      unwrap(api.GET("/api/v1/medleys/{id}/candidates", { params: { path: { id }, query: { kind } } })),
  });
}

/** A list of tracks to pick one from; it goes back to its placeholder once picked. */
function ChoiceSelect({
  label,
  placeholder,
  choices,
  onChoose,
}: {
  label: string;
  placeholder: string;
  choices: PartView[];
  onChoose: (chosen: PartView) => void;
}) {
  return (
    <select
      className="input"
      style={{ maxWidth: "16rem" }}
      aria-label={label}
      value=""
      disabled={choices.length === 0}
      onChange={(event) => {
        const chosen = choices.find((choice) => String(choice.track_id) === event.target.value);
        if (chosen) {
          onChoose(chosen);
        }
      }}
    >
      <option value="">{placeholder}</option>
      {choices.map((choice) => (
        <option key={choice.track_id} value={choice.track_id}>
          {trackName(choice)}
        </option>
      ))}
    </select>
  );
}
