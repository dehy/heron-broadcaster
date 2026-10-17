// SPDX-License-Identifier: AGPL-3.0-or-later

// Categories: groups of tracks the clockwheels will call (M6).

import { useQuery } from "@tanstack/react-query";
import { type FormEvent, useState } from "react";
import { type Category, api, done, unwrap } from "../api";
import { Card, ConfirmButton, Empty, ErrorView, Field, Icon, Loading, Modal, PageHead } from "../components/ui";
import { count } from "../lib/format";
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
    { success: "Category deleted", invalidate: [["categories"]] },
  );

  return (
    <>
      <PageHead
        title="Categories"
        description="Groups of tracks (current hits, recurrents, jingles…) that the clockwheels call. A track can be in several."
        actions={
          canEdit && (
            <button type="button" className="btn primary" onClick={() => setEditing("new")}>
              <Icon name="plus" /> New category
            </button>
          )
        }
      />
      <Card flush>
        {categories.isPending && <Loading />}
        {categories.isError && <ErrorView error={categories.error} />}
        {categories.data?.length === 0 && <Empty>No category yet.</Empty>}
        {categories.data && categories.data.length > 0 && (
          <table className="table">
            <thead>
              <tr>
                <th>Name</th>
                <th>Tracks</th>
                <th>
                  <span className="visually-hidden">Actions</span>
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
                    <Link to={`/library?category=${category.id}`}>{category.tracks.toLocaleString()}</Link>
                  </td>
                  <td className="actions">
                    {canEdit && (
                      <>
                        <button
                          type="button"
                          className="btn ghost small icon"
                          aria-label={`Edit ${category.name}`}
                          onClick={() => setEditing(category)}
                        >
                          <Icon name="edit" />
                        </button>
                        <ConfirmButton
                          className="btn ghost small icon danger"
                          title={`Delete ${category.name}`}
                          confirm="Delete"
                          question={
                            <p>
                              Delete <strong>{category.name}</strong>? Its {count(category.tracks, "track")} stay
                              in the library.
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
  const save = useAction(
    () => {
      const body = { name, color };
      return category
        ? unwrap(api.PUT("/api/v1/categories/{id}", { params: { path: { id: category.id } }, body }))
        : unwrap(api.POST("/api/v1/categories", { body }));
    },
    { success: "Saved", invalidate: [["categories"]], onSuccess: onClose },
  );
  const submit = (event: FormEvent) => {
    event.preventDefault();
    save.mutate(undefined);
  };
  return (
    <Modal
      title={category ? `Edit ${category.name}` : "New category"}
      open
      onClose={onClose}
      footer={
        <>
          <button type="button" className="btn" onClick={onClose}>
            Cancel
          </button>
          <button type="submit" form="category-form" className="btn primary" disabled={save.isPending}>
            Save
          </button>
        </>
      }
    >
      <form id="category-form" className="stack" onSubmit={submit}>
        <Field label="Name">
          <input className="input" value={name} onChange={(e) => setName(e.target.value)} required maxLength={255} />
        </Field>
        <Field label="Color">
          <div className="row">
            {COLORS.map((swatch) => (
              <button
                key={swatch}
                type="button"
                className="btn icon small"
                aria-label={`Color ${swatch}`}
                aria-pressed={color === swatch}
                style={{ background: swatch, borderColor: color === swatch ? "var(--fg)" : swatch }}
                onClick={() => setColor(swatch)}
              />
            ))}
            <input type="color" value={color} onChange={(e) => setColor(e.target.value)} aria-label="Other color" />
          </div>
        </Field>
      </form>
    </Modal>
  );
}
