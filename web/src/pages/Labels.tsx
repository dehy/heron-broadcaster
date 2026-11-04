// SPDX-License-Identifier: AGPL-3.0-or-later

// Labels: keywords on tracks to sort the library (christmas, instrumental,
// live). They live in Heron only; tracks get them from the library's
// selection actions.

import { type FormEvent, useState } from "react";
import { api, done, unwrap } from "../api";
import { LabelChip, type Label, useLabels } from "../components/SelectionActions";
import { Card, ConfirmButton, Empty, ErrorView, Field, Icon, Loading, Modal, PageHead } from "../components/ui";
import { number, rich, t } from "../i18n";
import { Link } from "../lib/router";
import { useAction, useCan } from "../lib/session";
import { COLORS } from "../lib/colors";

/** Queries a change of labels touches. */
const LABELS: string[][] = [["labels"], ["tracks"], ["track"]];

export function Labels() {
  const canEdit = useCan("operator");
  const labels = useLabels();
  const [editing, setEditing] = useState<Label | "new" | null>(null);
  const [merging, setMerging] = useState<Label | null>(null);
  const remove = useAction(
    (id: number) => done(api.DELETE("/api/v1/labels/{id}", { params: { path: { id } } })),
    { success: t("labels.deleted"), invalidate: LABELS },
  );
  return (
    <>
      <PageHead
        title={t("nav.labels")}
        description={rich("labels.description", {
          link: <Link to="/categories">{t("labels.categoriesLink")}</Link>,
        })}
        actions={
          canEdit && (
            <button type="button" className="btn primary" onClick={() => setEditing("new")}>
              <Icon name="plus" /> {t("labels.new")}
            </button>
          )
        }
      />
      <Card flush>
        {labels.isPending && <Loading />}
        {labels.isError && <ErrorView error={labels.error} />}
        {labels.data?.length === 0 && <Empty>{t("labels.none")}</Empty>}
        {labels.data && labels.data.length > 0 && (
          <table className="table">
            <thead>
              <tr>
                <th>{t("labels.label")}</th>
                <th>{t("nav.tracks")}</th>
                <th>
                  <span className="visually-hidden">{t("library.actions")}</span>
                </th>
              </tr>
            </thead>
            <tbody>
              {labels.data.map((label) => (
                <tr key={label.id}>
                  <td>
                    <LabelChip label={label} />
                  </td>
                  <td className="num">
                    <Link to={`/library?labels_any=${label.id}`}>{number(label.tracks)}</Link>
                  </td>
                  <td className="actions">
                    {canEdit && (
                      <>
                        <button
                          type="button"
                          className="btn ghost small icon"
                          aria-label={t("categories.edit", { name: label.name })}
                          title={t("labels.editHelp")}
                          onClick={() => setEditing(label)}
                        >
                          <Icon name="edit" />
                        </button>
                        <button
                          type="button"
                          className="btn ghost small"
                          disabled={labels.data.length < 2}
                          onClick={() => setMerging(label)}
                        >
                          {t("labels.merge")}
                        </button>
                        <ConfirmButton
                          className="btn ghost small icon danger"
                          title={t("categories.delete", { name: label.name })}
                          confirm={t("categories.deleteButton")}
                          question={
                            <p>
                              {rich("labels.deleteQuestion", {
                                name: <strong>{label.name}</strong>,
                                count: label.tracks,
                              })}
                            </p>
                          }
                          onConfirm={() => remove.mutate(label.id)}
                        >
                          <Icon name="trash" />
                        </ConfirmButton>
                      </>
                    )}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </Card>
      {editing && <LabelDialog label={editing === "new" ? null : editing} onClose={() => setEditing(null)} />}
      {merging && <MergeDialog label={merging} labels={labels.data ?? []} onClose={() => setMerging(null)} />}
    </>
  );
}

function LabelDialog({ label, onClose }: { label: Label | null; onClose: () => void }) {
  const [name, setName] = useState(label?.name ?? "");
  const [color, setColor] = useState(label?.color ?? COLORS[0] ?? "#2f6fd6");
  const save = useAction(
    () => {
      const body = { name, color };
      return label
        ? unwrap(api.PUT("/api/v1/labels/{id}", { params: { path: { id: label.id } }, body }))
        : unwrap(api.POST("/api/v1/labels", { body }));
    },
    { success: t("categories.saved"), invalidate: LABELS, onSuccess: onClose },
  );
  const submit = (event: FormEvent) => {
    event.preventDefault();
    save.mutate(undefined);
  };
  return (
    <Modal
      title={label ? t("categories.edit", { name: label.name }) : t("labels.new")}
      open
      onClose={onClose}
      footer={
        <>
          <button type="button" className="btn" onClick={onClose}>
            {t("common.cancel")}
          </button>
          <button type="submit" form="label-form" className="btn primary" disabled={save.isPending}>
            {t("common.save")}
          </button>
        </>
      }
    >
      <form id="label-form" className="stack" onSubmit={submit}>
        <Field label={t("categories.name")} hint={t("labels.nameHint")}>
          <input className="input" value={name} onChange={(e) => setName(e.target.value)} required maxLength={100} />
        </Field>
        <Field label={t("categories.color")}>
          <div className="row">
            {COLORS.map((swatch) => (
              <button
                key={swatch}
                type="button"
                className="btn icon small"
                aria-label={t("categories.colorValue", { color: swatch })}
                aria-pressed={color === swatch}
                style={{ background: swatch, borderColor: color === swatch ? "var(--fg)" : swatch }}
                onClick={() => setColor(swatch)}
              />
            ))}
            <input
              type="color"
              value={color}
              onChange={(e) => setColor(e.target.value)}
              aria-label={t("categories.otherColor")}
            />
          </div>
        </Field>
      </form>
    </Modal>
  );
}

function MergeDialog({ label, labels, onClose }: { label: Label; labels: Label[]; onClose: () => void }) {
  const others = labels.filter((other) => other.id !== label.id);
  const [into, setInto] = useState(String(others[0]?.id ?? ""));
  const merge = useAction(
    () =>
      unwrap(
        api.POST("/api/v1/labels/{id}/merge", {
          params: { path: { id: label.id } },
          body: { into_id: Number(into) },
        }),
      ),
    { success: t("labels.merged"), invalidate: LABELS, onSuccess: onClose },
  );
  return (
    <Modal
      title={t("labels.mergeTitle", { name: label.name })}
      open
      onClose={onClose}
      footer={
        <>
          <button type="button" className="btn" onClick={onClose}>
            {t("common.cancel")}
          </button>
          <button
            type="button"
            className="btn primary"
            disabled={!into || merge.isPending}
            onClick={() => merge.mutate(undefined)}
          >
            {t("labels.mergeButton")}
          </button>
        </>
      }
    >
      <div className="stack">
        <Field label={t("labels.into")}>
          <select className="input" value={into} onChange={(event) => setInto(event.target.value)}>
            {others.map((other) => (
              <option key={other.id} value={other.id}>
                {other.name}
              </option>
            ))}
          </select>
        </Field>
        <p className="muted small" style={{ margin: 0 }}>
          {t("labels.mergeHelp", { count: label.tracks, name: label.name })}
        </p>
      </div>
    </Modal>
  );
}
