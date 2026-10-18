// SPDX-License-Identifier: AGPL-3.0-or-later

// Restrictions: hard rules on tracks, artists and categories, never
// relaxed. Dayparting, date ranges and sequences.

import { useQuery } from "@tanstack/react-query";
import { type FormEvent, useState } from "react";
import { type RestrictionView, type Rule, type Target, api, done, query, unwrap } from "../api";
import { ArtistPicker } from "../components/ArtistPicker";
import { TrackSearch } from "../components/TrackSearch";
import {
  Card,
  ConfirmButton,
  Empty,
  ErrorView,
  Field,
  Icon,
  Loading,
  Modal,
  PageHead,
  Segmented,
} from "../components/ui";
import { trackName } from "../lib/format";
import { DAYS, describeRule, hourLabel, targetKind } from "../lib/rotation";
import { Link, setSearch, useLocation } from "../lib/router";
import { useAction, useCan } from "../lib/session";
import { useCategories } from "./Clocks";

type TargetKind = Target["kind"];

function targetLink(target: Target): string {
  switch (target.kind) {
    case "track":
      return `/library/${target.id}`;
    case "artist":
      return `/artists?artist=${target.id}`;
    case "category":
      return `/library?category=${target.id}`;
  }
}

export function Restrictions() {
  const canEdit = useCan("operator");
  const { search } = useLocation();
  const kind = (search.get("target") as TargetKind | null) ?? undefined;
  const targetId = search.get("target_id") ? Number(search.get("target_id")) : undefined;
  const restrictions = useQuery({
    queryKey: ["restrictions", kind, targetId],
    queryFn: () =>
      unwrap(api.GET("/api/v1/restrictions", { params: { query: query({ target: kind, target_id: targetId }) } })),
  });
  const categories = useCategories();
  const categoryName = (id: number) => categories.data?.find((c) => c.id === id)?.name;
  const [editing, setEditing] = useState<RestrictionView | "new" | null>(null);
  const toggle = useAction(
    (restriction: RestrictionView) =>
      unwrap(
        api.PUT("/api/v1/restrictions/{id}", {
          params: { path: { id: restriction.id } },
          body: {
            target: restriction.target,
            rule: restriction.rule,
            enabled: !restriction.enabled,
            note: restriction.note ?? null,
          },
        }),
      ),
    { invalidate: [["restrictions"]] },
  );
  const remove = useAction(
    (id: number) => done(api.DELETE("/api/v1/restrictions/{id}", { params: { path: { id } } })),
    { success: "Restriction deleted", invalidate: [["restrictions"]] },
  );

  return (
    <>
      <PageHead
        title="Restrictions"
        description="Hard rules: unlike separation, they are never relaxed. When a slot finds nothing they allow, it uses the fallback category; if even that is forbidden, the engine's fallback directory plays."
        actions={
          canEdit && (
            <button type="button" className="btn primary" onClick={() => setEditing("new")}>
              <Icon name="plus" /> New restriction
            </button>
          )
        }
      />
      {(kind || targetId) && (
        <p className="row small">
          <span className="muted">Showing the restrictions of one {kind ?? "target"}.</span>
          <button type="button" className="link-button" onClick={() => setSearch({ target: null, target_id: null })}>
            Show all
          </button>
        </p>
      )}
      <Card flush>
        {restrictions.isPending && <Loading />}
        {restrictions.isError && <ErrorView error={restrictions.error} />}
        {restrictions.data?.length === 0 && <Empty>No restriction.</Empty>}
        {restrictions.data && restrictions.data.length > 0 && (
          <div className="table-wrap">
            <table className="table">
              <thead>
                <tr>
                  <th>On</th>
                  <th>Rule</th>
                  <th>Note</th>
                  <th>Active</th>
                  <th>
                    <span className="visually-hidden">Actions</span>
                  </th>
                </tr>
              </thead>
              <tbody>
                {restrictions.data.map((restriction) => (
                  <tr key={restriction.id} className={restriction.enabled ? undefined : "muted"}>
                    <td className="title-cell">
                      <span className="badge">{targetKind(restriction.target)}</span>{" "}
                      <Link to={targetLink(restriction.target)}>
                        {restriction.target_name ?? `#${restriction.target.id}`}
                      </Link>
                    </td>
                    <td>{describeRule(restriction.rule, categoryName)}</td>
                    <td className="muted small">{restriction.note}</td>
                    <td>
                      <input
                        type="checkbox"
                        aria-label="Active"
                        checked={restriction.enabled}
                        disabled={!canEdit || toggle.isPending}
                        onChange={() => toggle.mutate(restriction)}
                      />
                    </td>
                    <td className="actions">
                      {canEdit && (
                        <>
                          <button
                            type="button"
                            className="btn ghost small icon"
                            aria-label="Edit"
                            onClick={() => setEditing(restriction)}
                          >
                            <Icon name="edit" />
                          </button>
                          <ConfirmButton
                            className="btn ghost small icon danger"
                            title="Delete"
                            confirm="Delete"
                            question={<p>Delete this restriction?</p>}
                            onConfirm={() => remove.mutate(restriction.id)}
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
          </div>
        )}
      </Card>
      {editing && (
        <RestrictionDialog restriction={editing === "new" ? null : editing} onClose={() => setEditing(null)} />
      )}
    </>
  );
}

const ALL_DAYS = [1, 2, 3, 4, 5, 6, 7];

function defaultRule(kind: Rule["kind"], categoryId: number): Rule {
  const year = new Date().getFullYear();
  switch (kind) {
    case "daypart":
      return { kind, mode: "ban", days: ALL_DAYS, start_hour: 6, end_hour: 22 };
    case "date_range":
      return { kind, mode: "allow", start: `${year}-12-01`, end: `${year + 1}-01-06`, every_year: true };
    case "not_after":
      return { kind, category_id: categoryId };
  }
}

export function RestrictionDialog({
  restriction,
  target: fixedTarget,
  onClose,
}: {
  restriction: RestrictionView | null;
  /** For a new restriction on a known target. */
  target?: { target: Target; name: string };
  onClose: () => void;
}) {
  const categories = useCategories();
  const firstCategory = categories.data?.[0]?.id ?? 0;
  const [target, setTarget] = useState<Target | null>(restriction?.target ?? fixedTarget?.target ?? null);
  const [targetName, setTargetName] = useState(restriction?.target_name ?? fixedTarget?.name ?? "");
  const [kind, setKind] = useState<TargetKind>(target?.kind ?? "category");
  const [rule, setRule] = useState<Rule>(restriction?.rule ?? defaultRule("daypart", firstCategory));
  const [note, setNote] = useState(restriction?.note ?? "");
  const save = useAction(
    () => {
      if (!target) {
        throw new Error("Pick what the restriction applies to");
      }
      const body = { target, rule, enabled: restriction?.enabled ?? true, note: note.trim() || null };
      return restriction
        ? unwrap(api.PUT("/api/v1/restrictions/{id}", { params: { path: { id: restriction.id } }, body }))
        : unwrap(api.POST("/api/v1/restrictions", { body }));
    },
    { success: "Restriction saved", invalidate: [["restrictions"]], onSuccess: onClose },
  );
  const submit = (event: FormEvent) => {
    event.preventDefault();
    save.mutate(undefined);
  };
  const pick = (next: Target, name: string) => {
    setTarget(next);
    setTargetName(name);
  };

  return (
    <Modal
      title={restriction ? "Edit restriction" : "New restriction"}
      open
      wide
      onClose={onClose}
      footer={
        <>
          <button type="button" className="btn" onClick={onClose}>
            Cancel
          </button>
          <button type="submit" form="restriction-form" className="btn primary" disabled={!target || save.isPending}>
            Save
          </button>
        </>
      }
    >
      <form id="restriction-form" className="stack" onSubmit={submit}>
        <Field label="Applies to">
          {fixedTarget ? (
            <span>
              <span className="badge">{targetKind(fixedTarget.target)}</span> {fixedTarget.name}
            </span>
          ) : (
            <div className="stack">
              <Segmented
                label="Kind of target"
                value={kind}
                options={[
                  { value: "category", label: "Category" },
                  { value: "artist", label: "Artist" },
                  { value: "track", label: "Track" },
                ]}
                onChange={(next) => {
                  setKind(next);
                  setTarget(null);
                  setTargetName("");
                }}
              />
              {kind === "category" && (
                <select
                  className="input"
                  aria-label="Category"
                  value={target?.kind === "category" ? target.id : ""}
                  onChange={(event) => {
                    const id = Number(event.target.value);
                    pick({ kind: "category", id }, categories.data?.find((c) => c.id === id)?.name ?? "");
                  }}
                >
                  <option value="" disabled>
                    Pick a category
                  </option>
                  {categories.data?.map((c) => (
                    <option key={c.id} value={c.id}>
                      {c.name}
                    </option>
                  ))}
                </select>
              )}
              {kind !== "category" && target && (
                <div className="row">
                  <strong>{targetName}</strong>
                  <button
                    type="button"
                    className="link-button"
                    onClick={() => {
                      setTarget(null);
                      setTargetName("");
                    }}
                  >
                    Change
                  </button>
                </div>
              )}
              {kind === "artist" && !target && (
                <ArtistPicker
                  actions={(artist) => (
                    <button
                      type="button"
                      className="btn small"
                      onClick={() => pick({ kind: "artist", id: artist.id }, artist.name)}
                    >
                      Pick
                    </button>
                  )}
                />
              )}
              {kind === "track" && !target && (
                <TrackSearch
                  playableOnly={false}
                  actions={(track) => (
                    <button
                      type="button"
                      className="btn small"
                      onClick={() => pick({ kind: "track", id: track.id }, trackName(track))}
                    >
                      Pick
                    </button>
                  )}
                />
              )}
            </div>
          )}
        </Field>
        <Field label="Rule">
          <Segmented
            label="Kind of rule"
            value={rule.kind}
            options={[
              { value: "daypart", label: "Days and hours" },
              { value: "date_range", label: "Dates" },
              { value: "not_after", label: "Sequence" },
            ]}
            onChange={(next) => setRule(next === rule.kind ? rule : defaultRule(next, firstCategory))}
          />
        </Field>
        <RuleFields rule={rule} onChange={setRule} />
        <p className="muted small" style={{ margin: 0 }}>
          {describeRule(rule, (id) => categories.data?.find((c) => c.id === id)?.name)}.
        </p>
        <Field label="Note" hint="Why the rule is there, for the team">
          <input className="input" value={note} maxLength={512} onChange={(event) => setNote(event.target.value)} />
        </Field>
      </form>
    </Modal>
  );
}

function RuleFields({ rule, onChange }: { rule: Rule; onChange: (rule: Rule) => void }) {
  const categories = useCategories();
  const mode =
    rule.kind === "not_after" ? null : (
      <Segmented
        label="Allow or ban"
        value={rule.mode}
        options={[
          { value: "ban", label: "Banned then" },
          { value: "allow", label: "Only then" },
        ]}
        onChange={(next) => onChange({ ...rule, mode: next })}
      />
    );
  switch (rule.kind) {
    case "daypart":
      return (
        <div className="stack">
          {mode}
          <fieldset>
            <legend>Days</legend>
            <div className="row">
              {DAYS.map((name, index) => {
                const day = index + 1;
                return (
                  <label key={day} className="check">
                    <input
                      type="checkbox"
                      checked={rule.days.includes(day)}
                      onChange={(event) =>
                        onChange({
                          ...rule,
                          days: event.target.checked
                            ? [...rule.days, day].sort((a, b) => a - b)
                            : rule.days.filter((d) => d !== day),
                        })
                      }
                    />
                    {name}
                  </label>
                );
              })}
            </div>
          </fieldset>
          <div className="form-grid">
            <Field label="From">
              <select
                className="input"
                value={rule.start_hour}
                onChange={(event) => onChange({ ...rule, start_hour: Number(event.target.value) })}
              >
                {Array.from({ length: 24 }, (_, hour) => (
                  <option key={hour} value={hour}>
                    {hourLabel(hour)}
                  </option>
                ))}
              </select>
            </Field>
            <Field label="To" hint={rule.end_hour <= rule.start_hour ? "Past midnight, into the next day" : undefined}>
              <select
                className="input"
                value={rule.end_hour}
                onChange={(event) => onChange({ ...rule, end_hour: Number(event.target.value) })}
              >
                {Array.from({ length: 24 }, (_, index) => index + 1).map((hour) => (
                  <option key={hour} value={hour}>
                    {hour === 24 ? "24:00" : hourLabel(hour)}
                  </option>
                ))}
              </select>
            </Field>
          </div>
        </div>
      );
    case "date_range":
      return (
        <div className="stack">
          {mode}
          <div className="form-grid">
            <Field label="From">
              <input
                className="input"
                type="date"
                value={rule.start}
                required
                onChange={(event) => onChange({ ...rule, start: event.target.value })}
              />
            </Field>
            <Field label="To" hint="Included">
              <input
                className="input"
                type="date"
                value={rule.end}
                required
                onChange={(event) => onChange({ ...rule, end: event.target.value })}
              />
            </Field>
            <label className="check">
              <input
                type="checkbox"
                checked={rule.every_year}
                onChange={(event) => onChange({ ...rule, every_year: event.target.checked })}
              />
              Every year (the years are ignored)
            </label>
          </div>
        </div>
      );
    case "not_after":
      return (
        <Field label="Never directly after a track of" hint="Pick the target's own category for “no two in a row”">
          <select
            className="input"
            value={rule.category_id}
            onChange={(event) => onChange({ ...rule, category_id: Number(event.target.value) })}
          >
            {categories.data?.map((c) => (
              <option key={c.id} value={c.id}>
                {c.name}
              </option>
            ))}
          </select>
        </Field>
      );
  }
}
