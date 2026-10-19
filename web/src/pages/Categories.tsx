// SPDX-License-Identifier: AGPL-3.0-or-later

// Categories: groups of tracks the clocks call, with the separation
// windows they override.

import { useQuery } from "@tanstack/react-query";
import { type FormEvent, useState } from "react";
import { type Category, type SeparationOverrides, api, done, unwrap } from "../api";
import { SeparationEditor } from "../components/Separation";
import { Card, ConfirmButton, Empty, ErrorView, Field, Icon, Loading, Modal, PageHead } from "../components/ui";
import { number, rich, t } from "../i18n";
import { describeOverrides } from "../lib/rotation";
import { Link } from "../lib/router";
import { useAction, useCan } from "../lib/session";

const COLORS = ["#2f6fd6", "#b4549a", "#1d7a3e", "#b07a00", "#c62828", "#0b6e66", "#6b4fbb", "#5f6268"];

export function Categories() {
  const canEdit = useCan("operator");
  const categories = useQuery({
    queryKey: ["categories"],
    queryFn: () => unwrap(api.GET("/api/v1/categories")),
  });
  const [editing, setEditing] = useState<Category | "new" | null>(null);
  const remove = useAction(
    (id: number) => done(api.DELETE("/api/v1/categories/{id}", { params: { path: { id } } })),
    { success: t("categories.deleted"), invalidate: [["categories"]] },
  );

  return (
    <>
      <PageHead
        title={t("nav.categories")}
        description={t("categories.description")}
        actions={
          canEdit && (
            <button type="button" className="btn primary" onClick={() => setEditing("new")}>
              <Icon name="plus" /> {t("categories.new")}
            </button>
          )
        }
      />
      <Card flush>
        {categories.isPending && <Loading />}
        {categories.isError && <ErrorView error={categories.error} />}
        {categories.data?.length === 0 && <Empty>{t("categories.none")}</Empty>}
        {categories.data && categories.data.length > 0 && (
          <table className="table">
            <thead>
              <tr>
                <th>{t("categories.name")}</th>
                <th>{t("nav.tracks")}</th>
                <th>{t("categories.separation")}</th>
                <th>
                  <span className="visually-hidden">{t("library.actions")}</span>
                </th>
              </tr>
            </thead>
            <tbody>
              {categories.data.map((category) => (
                <tr key={category.id}>
                  <td>
                    <span className="row">
                      <span className="swatch" style={{ background: category.color ?? undefined }} />
                      {category.name}
                    </span>
                  </td>
                  <td className="num">
                    <Link to={`/library?category=${category.id}`}>{number(category.tracks)}</Link>
                  </td>
                  <td className="small muted">{describeOverrides(category.separation) || t("categories.stationRules")}</td>
                  <td className="actions">
                    {canEdit && (
                      <>
                        <button
                          type="button"
                          className="btn ghost small icon"
                          aria-label={t("categories.edit", { name: category.name })}
                          onClick={() => setEditing(category)}
                        >
                          <Icon name="edit" />
                        </button>
                        <ConfirmButton
                          className="btn ghost small icon danger"
                          title={t("categories.delete", { name: category.name })}
                          confirm={t("categories.deleteButton")}
                          question={
                            <p>
                              {rich("categories.deleteQuestion", {
                                name: <strong>{category.name}</strong>,
                                count: category.tracks,
                              })}
                            </p>
                          }
                          onConfirm={() => remove.mutate(category.id)}
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
      {editing && <CategoryDialog category={editing === "new" ? null : editing} onClose={() => setEditing(null)} />}
    </>
  );
}

function CategoryDialog({ category, onClose }: { category: Category | null; onClose: () => void }) {
  const [name, setName] = useState(category?.name ?? "");
  const [color, setColor] = useState(category?.color ?? COLORS[0] ?? "#2f6fd6");
  const [separation, setSeparation] = useState<SeparationOverrides>(category?.separation ?? {});
  const rotation = useQuery({
    queryKey: ["settings", "rotation"],
    queryFn: () => unwrap(api.GET("/api/v1/settings/rotation")),
  });
  const save = useAction(
    () => {
      const body = { name, color, separation };
      return category
        ? unwrap(api.PUT("/api/v1/categories/{id}", { params: { path: { id: category.id } }, body }))
        : unwrap(api.POST("/api/v1/categories", { body }));
    },
    { success: t("categories.saved"), invalidate: [["categories"]], onSuccess: onClose },
  );
  const submit = (event: FormEvent) => {
    event.preventDefault();
    save.mutate(undefined);
  };
  return (
    <Modal
      title={category ? t("categories.edit", { name: category.name }) : t("categories.new")}
      open
      onClose={onClose}
      footer={
        <>
          <button type="button" className="btn" onClick={onClose}>
            {t("common.cancel")}
          </button>
          <button type="submit" form="category-form" className="btn primary" disabled={save.isPending}>
            {t("common.save")}
          </button>
        </>
      }
    >
      <form id="category-form" className="stack" onSubmit={submit}>
        <Field label={t("categories.name")}>
          <input className="input" value={name} onChange={(e) => setName(e.target.value)} required maxLength={255} />
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
            <input type="color" value={color} onChange={(e) => setColor(e.target.value)} aria-label={t("categories.otherColor")} />
          </div>
        </Field>
        <fieldset>
          <legend>{t("categories.separation")}</legend>
          <SeparationEditor value={separation} inherited={rotation.data?.separation} onChange={setSeparation} />
        </fieldset>
      </form>
    </Modal>
  );
}
