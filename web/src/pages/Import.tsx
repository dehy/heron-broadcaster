// SPDX-License-Identifier: AGPL-3.0-or-later

// Importing track details from another radio automation software: a JSON
// Lines export (one audio file per line) is matched to the library, the
// categories and problems get their names in Heron, every conflict with a
// value set in Heron is settled, then it is applied as one import that can be
// undone. See docs/import.md.

import { useQuery, useQueryClient } from "@tanstack/react-query";
import { type ChangeEvent, useMemo, useState } from "react";
import { type Schemas, api, unwrap } from "../api";
import { Banner, Card, ConfirmButton, Empty, Icon, Loading, PageHead, Segmented } from "../components/ui";
import { number, t } from "../i18n";
import { dateTime, duration } from "../lib/format";
import { Link } from "../lib/router";
import { useAction } from "../lib/session";
import { useCategoryTypes } from "./CategoryTypes";

type SourceRow = Schemas["ImportSourceRow"];
type Preview = Schemas["ImportPreview"];
type Row = Schemas["ImportRow"];
type Values = Schemas["ImportValues"];
type Field = Schemas["ImportField"];
type ProblemAction = Schemas["ImportProblemAction"];

/** How a row's conflicts are settled. */
type Resolution = { kind: "import" } | { kind: "heron" } | { kind: "edit"; values: Partial<Record<Field, string>> };

/** `type_id` undefined: the type guessed from the export (Jingle, else Song). */
type CategoryChoice = { name: string; fx_player: boolean; disable_tracks: boolean; type_id?: number | null };
type ProblemChoice = { action: ProblemAction; label: string };

type Filter = "matched" | "conflicts" | "length" | "warnings" | "skipped";

/** Rows shown at once in the table. */
const SHOWN = 200;

const VALUE_KEY: Record<Field, keyof Values> = {
  title: "title",
  artist: "artist",
  year: "year",
  fade_in: "fade_in_ms",
  fade_out: "fade_out_ms",
  intro: "intro_ms",
  intro2: "intro2_ms",
  hook_in: "hook_in_ms",
  hook_out: "hook_out_ms",
  next_soft: "next_soft_ms",
  next_hard: "next_hard_ms",
};
const TEXT_FIELDS: Field[] = ["title", "artist", "year"];

function fieldLabel(field: Field): string {
  return t(`import.field.${field}`);
}

/** A value as shown: text, or a time in seconds. */
function show(field: Field, values: Values): string {
  const value = values[VALUE_KEY[field]];
  if (value === null || value === undefined) {
    return "–";
  }
  return TEXT_FIELDS.includes(field) ? String(value) : duration(Number(value), true);
}

/** Reads a JSON Lines export (or a JSON array). */
function parse(text: string): { rows: SourceRow[]; errors: number[] } {
  const trimmed = text.trim();
  if (trimmed.startsWith("[")) {
    return { rows: JSON.parse(trimmed) as SourceRow[], errors: [] };
  }
  const rows: SourceRow[] = [];
  const errors: number[] = [];
  trimmed.split(/\r?\n/).forEach((line, index) => {
    if (!line.trim()) {
      return;
    }
    try {
      rows.push(JSON.parse(line) as SourceRow);
    } catch {
      errors.push(index + 1);
    }
  });
  return { rows, errors };
}

/** The values to apply for a row, its conflicts settled. */
function resolved(row: Row, resolution: Resolution | undefined): Values {
  const values: Values = { ...row.values };
  for (const field of row.conflicts) {
    const key = VALUE_KEY[field];
    if (resolution?.kind === "heron") {
      delete values[key];
    } else if (resolution?.kind === "edit") {
      const edited = resolution.values[field]?.trim();
      if (!edited) {
        delete values[key];
      } else if (TEXT_FIELDS.includes(field)) {
        (values as Record<string, unknown>)[key] = edited;
      } else {
        (values as Record<string, unknown>)[key] = Math.round(Number(edited.replace(",", ".")) * 1000);
      }
    }
  }
  return values;
}

export function Import() {
  const client = useQueryClient();
  const [fileName, setFileName] = useState<string | null>(null);
  const [parseErrors, setParseErrors] = useState<number[]>([]);
  const [preview, setPreview] = useState<Preview | null>(null);
  const [categories, setCategories] = useState<Record<number, CategoryChoice>>({});
  const [problems, setProblems] = useState<Record<number, ProblemChoice>>({});
  const [excluded, setExcluded] = useState<Set<number>>(new Set());
  const [resolutions, setResolutions] = useState<Record<number, Resolution>>({});
  const [filter, setFilter] = useState<Filter>("matched");
  const [result, setResult] = useState<Schemas["TrackImport"] | null>(null);

  const types = useCategoryTypes();
  const typeNamed = (name: string) =>
    types.data?.find((categoryType) => categoryType.name.toLowerCase() === name)?.id ?? null;
  const typeOf = (choice: CategoryChoice, isJingle: boolean) =>
    choice.type_id !== undefined ? choice.type_id : typeNamed(isJingle ? "jingle" : "song");

  const history = useQuery({
    queryKey: ["imports"],
    queryFn: () => unwrap(api.GET("/api/v1/library/imports")),
  });

  const load = useAction(
    (rows: SourceRow[]) => unwrap(api.POST("/api/v1/library/import/preview", { body: { rows } })),
    {
      onSuccess: (data) => {
        setPreview(data);
        setResult(null);
        setResolutions({});
        // Rows whose file is another length are left out until checked.
        setExcluded(new Set(data.rows.filter((row) => row.length_mismatch).map((row) => row.line)));
        setCategories(
          Object.fromEntries(
            data.categories.map((category) => [
              category.id,
              { name: category.name, fx_player: category.is_jingle, disable_tracks: !category.can_schedule },
            ]),
          ),
        );
        setProblems(
          Object.fromEntries(data.problems.map((problem) => [problem.id, { action: "label", label: problem.name }])),
        );
      },
    },
  );

  const onFile = async (event: ChangeEvent<HTMLInputElement>) => {
    const file = event.target.files?.[0];
    event.target.value = "";
    if (!file) {
      return;
    }
    setFileName(file.name);
    try {
      const { rows, errors } = parse(await file.text());
      setParseErrors(errors);
      load.mutate(rows);
    } catch {
      setParseErrors([0]);
    }
  };

  const matched = useMemo(() => preview?.rows.filter((row) => row.status === "matched") ?? [], [preview]);
  const included = matched.filter((row) => !excluded.has(row.line));
  const unsettled = included.filter((row) => row.conflicts.length > 0 && !resolutions[row.line]);
  const counts = useMemo(() => {
    const rows = preview?.rows ?? [];
    return {
      matched: matched.length,
      conflicts: matched.filter((row) => row.conflicts.length > 0).length,
      length: matched.filter((row) => row.length_mismatch).length,
      warnings: matched.filter((row) => row.warnings.length > 0).length,
      skipped: rows.length - matched.length,
    };
  }, [preview, matched]);

  const shown = (preview?.rows ?? []).filter((row) => {
    switch (filter) {
      case "matched":
        return row.status === "matched";
      case "conflicts":
        return row.status === "matched" && row.conflicts.length > 0;
      case "length":
        return row.status === "matched" && row.length_mismatch;
      case "warnings":
        return row.status === "matched" && row.warnings.length > 0;
      case "skipped":
        return row.status !== "matched";
    }
  });

  const settleAll = (kind: "import" | "heron") =>
    setResolutions((all) => {
      const next = { ...all };
      for (const row of matched) {
        if (row.conflicts.length > 0) {
          next[row.line] = { kind };
        }
      }
      return next;
    });

  const apply = useAction(
    () =>
      unwrap(
        api.POST("/api/v1/library/import/apply", {
          body: {
            categories: Object.entries(categories).map(([id, choice]) => ({
              id: Number(id),
              name: choice.name.trim() || null,
              type_id: typeOf(choice, preview?.categories.find((c) => c.id === Number(id))?.is_jingle ?? false),
              fx_player: choice.fx_player,
              disable_tracks: choice.disable_tracks,
            })),
            problems: Object.entries(problems).map(([id, choice]) => ({
              id: Number(id),
              action: choice.action,
              label: choice.label.trim() || null,
            })),
            rows: included.map((row) => ({
              track_id: row.track_id ?? 0,
              values: resolved(row, resolutions[row.line]),
            })),
          },
        }),
      ),
    {
      invalidate: [["imports"], ["tracks"], ["track"], ["categories"], ["labels"]],
      onSuccess: (data) => {
        setResult(data);
        setPreview(null);
      },
    },
  );

  const undo = useAction((id: number) => unwrap(api.POST("/api/v1/library/imports/{id}/undo", { params: { path: { id } } })), {
    success: t("import.undone"),
    invalidate: [["imports"], ["tracks"], ["track"]],
  });

  return (
    <>
      <PageHead title={t("import.title")} description={t("import.description")} />

      <Card title={t("import.fileTitle")}>
        <div className="stack">
          <p className="muted small" style={{ margin: 0 }}>
            {t("import.fileHelp")}
          </p>
          <div className="row">
            <label className="btn">
              <Icon name="upload" size={14} /> {t("import.chooseFile")}
              <input
                type="file"
                accept=".jsonl,.ndjson,.json,application/json"
                onChange={(event) => void onFile(event)}
                hidden
              />
            </label>
            {fileName && <span className="muted">{fileName}</span>}
            {load.isPending && <Loading label={t("import.matching")} />}
          </div>
          {parseErrors.length > 0 && (
            <Banner kind="bad">
              {parseErrors[0] === 0
                ? t("import.unreadable")
                : t("import.badLines", { count: parseErrors.length, lines: parseErrors.slice(0, 10).join(", ") })}
            </Banner>
          )}
        </div>
      </Card>

      {result && (
        <Banner kind="warn">
          {t("import.done", {
            count: result.summary.tracks,
            disabled: result.summary.disabled,
            categories: result.summary.categories_created.length,
            labels: result.summary.labels_created.length,
          })}{" "}
          <Link to="/library">{t("import.seeTracks")}</Link>
        </Banner>
      )}

      {preview && (
        <>
          <Card title={t("import.categoriesTitle")}>
            {preview.categories.length === 0 ? (
              <Empty>{t("import.noCategories")}</Empty>
            ) : (
              <div className="stack">
                <p className="muted small" style={{ margin: 0 }}>
                  {t("import.categoriesHelp")}
                </p>
                <div className="table-wrap">
                  <table className="table">
                    <thead>
                      <tr>
                        <th>{t("import.exportCategory")}</th>
                        <th className="num">{t("import.rows")}</th>
                        <th>{t("import.heronCategory")}</th>
                        <th>{t("categories.type")}</th>
                        <th>{t("import.fxPlayer")}</th>
                        <th>{t("import.disableTracks")}</th>
                      </tr>
                    </thead>
                    <tbody>
                      {preview.categories.map((category) => {
                        const choice = categories[category.id];
                        const set = (change: Partial<CategoryChoice>) =>
                          setCategories((all) => (choice ? { ...all, [category.id]: { ...choice, ...change } } : all));
                        return (
                          choice && (
                            <tr key={category.id}>
                              <td>
                                {category.name}
                                {!category.can_schedule && <span className="badge warn">{t("import.notScheduled")}</span>}
                                {category.is_jingle && <span className="badge accent">{t("import.jingles")}</span>}
                              </td>
                              <td className="num">{number(category.rows)}</td>
                              <td>
                                <input
                                  className="input"
                                  value={choice.name}
                                  placeholder={t("import.noCategory")}
                                  aria-label={t("import.heronCategoryFor", { name: category.name })}
                                  onChange={(event) => set({ name: event.target.value })}
                                />
                                {category.heron_category_id !== null &&
                                  category.heron_category_id !== undefined &&
                                  choice.name === category.name && (
                                    <div className="muted small">{t("import.existing")}</div>
                                  )}
                              </td>
                              <td>
                                <select
                                  className="input"
                                  value={String(typeOf(choice, category.is_jingle) ?? "")}
                                  aria-label={t("import.typeFor", { name: category.name })}
                                  onChange={(event) =>
                                    set({ type_id: event.target.value ? Number(event.target.value) : null })
                                  }
                                >
                                  {types.data?.map((categoryType) => (
                                    <option key={categoryType.id} value={categoryType.id}>
                                      {categoryType.name}
                                    </option>
                                  ))}
                                  <option value="">{t("categories.noType")}</option>
                                </select>
                              </td>
                              <td>
                                <input
                                  type="checkbox"
                                  checked={choice.fx_player}
                                  aria-label={t("import.fxPlayer")}
                                  onChange={(event) => set({ fx_player: event.target.checked })}
                                />
                              </td>
                              <td>
                                <input
                                  type="checkbox"
                                  checked={choice.disable_tracks}
                                  aria-label={t("import.disableTracks")}
                                  onChange={(event) => set({ disable_tracks: event.target.checked })}
                                />
                              </td>
                            </tr>
                          )
                        );
                      })}
                    </tbody>
                  </table>
                </div>
              </div>
            )}
          </Card>

          {preview.problems.length > 0 && (
            <Card title={t("import.problemsTitle")}>
              <div className="stack">
                <p className="muted small" style={{ margin: 0 }}>
                  {t("import.problemsHelp")}
                </p>
                <div className="table-wrap">
                  <table className="table">
                    <thead>
                      <tr>
                        <th>{t("import.problem")}</th>
                        <th className="num">{t("import.rows")}</th>
                        <th>{t("import.action")}</th>
                        <th>{t("import.label")}</th>
                      </tr>
                    </thead>
                    <tbody>
                      {preview.problems.map((problem) => {
                        const choice = problems[problem.id];
                        const set = (change: Partial<ProblemChoice>) =>
                          setProblems((all) => (choice ? { ...all, [problem.id]: { ...choice, ...change } } : all));
                        return (
                          choice && (
                            <tr key={problem.id}>
                              <td>{problem.name}</td>
                              <td className="num">{number(problem.rows)}</td>
                              <td>
                                <select
                                  className="input"
                                  value={choice.action}
                                  aria-label={t("import.action")}
                                  onChange={(event) => set({ action: event.target.value as ProblemAction })}
                                >
                                  <option value="ignore">{t("import.action.ignore")}</option>
                                  <option value="label">{t("import.action.label")}</option>
                                  <option value="label_and_disable">{t("import.action.labelAndDisable")}</option>
                                </select>
                              </td>
                              <td>
                                <input
                                  className="input"
                                  value={choice.label}
                                  disabled={choice.action === "ignore"}
                                  aria-label={t("import.label")}
                                  onChange={(event) => set({ label: event.target.value })}
                                />
                              </td>
                            </tr>
                          )
                        );
                      })}
                    </tbody>
                  </table>
                </div>
              </div>
            </Card>
          )}

          <Card
            title={t("import.rowsTitle")}
            actions={
              counts.conflicts > 0 && (
                <div className="row">
                  <button type="button" className="btn small" onClick={() => settleAll("import")}>
                    {t("import.allImport")}
                  </button>
                  <button type="button" className="btn small" onClick={() => settleAll("heron")}>
                    {t("import.allHeron")}
                  </button>
                </div>
              )
            }
          >
            <div className="stack">
              <Segmented
                label={t("import.rowsTitle")}
                value={filter}
                onChange={setFilter}
                options={[
                  { value: "matched", label: t("import.filter.matched", { count: counts.matched }) },
                  { value: "conflicts", label: t("import.filter.conflicts", { count: counts.conflicts }) },
                  { value: "length", label: t("import.filter.length", { count: counts.length }) },
                  { value: "warnings", label: t("import.filter.warnings", { count: counts.warnings }) },
                  { value: "skipped", label: t("import.filter.skipped", { count: counts.skipped }) },
                ]}
              />
              {shown.length === 0 ? (
                <Empty>{t("import.noRows")}</Empty>
              ) : (
                <div className="table-wrap">
                  <table className="table">
                    <thead>
                      <tr>
                        <th>{t("import.include")}</th>
                        <th>{t("import.file")}</th>
                        <th>{t("import.changes")}</th>
                      </tr>
                    </thead>
                    <tbody>
                      {shown.slice(0, SHOWN).map((row) => (
                        <ImportRowView
                          key={row.line}
                          row={row}
                          included={row.status === "matched" && !excluded.has(row.line)}
                          onInclude={(include) =>
                            setExcluded((all) => {
                              const next = new Set(all);
                              if (include) {
                                next.delete(row.line);
                              } else {
                                next.add(row.line);
                              }
                              return next;
                            })
                          }
                          resolution={resolutions[row.line]}
                          onResolve={(resolution) => setResolutions((all) => ({ ...all, [row.line]: resolution }))}
                        />
                      ))}
                    </tbody>
                  </table>
                  {shown.length > SHOWN && (
                    <p className="muted small">{t("import.more", { count: shown.length - SHOWN })}</p>
                  )}
                </div>
              )}
              <div className="row between">
                <span className="muted">
                  {unsettled.length > 0
                    ? t("import.unsettled", { count: unsettled.length })
                    : t("import.ready", { count: included.length })}
                </span>
                <button
                  type="button"
                  className="btn primary"
                  disabled={included.length === 0 || unsettled.length > 0 || apply.isPending}
                  onClick={() => apply.mutate(undefined)}
                >
                  {t("import.apply", { count: included.length })}
                </button>
              </div>
            </div>
          </Card>
        </>
      )}

      <Card title={t("import.historyTitle")}>
        {history.isPending ? (
          <Loading />
        ) : !history.data?.length ? (
          <Empty>{t("import.noHistory")}</Empty>
        ) : (
          <div className="table-wrap">
            <table className="table">
              <thead>
                <tr>
                  <th>{t("import.when")}</th>
                  <th>{t("import.by")}</th>
                  <th className="num">{t("import.tracks")}</th>
                  <th />
                </tr>
              </thead>
              <tbody>
                {history.data.map((item) => (
                  <tr key={item.id}>
                    <td>{dateTime(item.created_at_unix_ms)}</td>
                    <td>{item.created_by ?? "–"}</td>
                    <td className="num">{number(item.summary.tracks)}</td>
                    <td className="actions">
                      {item.undone_at_unix_ms ? (
                        <span className="muted">{t("import.undoneAt", { date: dateTime(item.undone_at_unix_ms) })}</span>
                      ) : (
                        <ConfirmButton
                          className="btn small"
                          confirm={t("import.undo")}
                          question={<p>{t("import.undoQuestion", { count: item.summary.tracks })}</p>}
                          disabled={undo.isPending}
                          onConfirm={() => {
                            undo.mutate(item.id);
                            void client.invalidateQueries({ queryKey: ["imports"] });
                          }}
                        >
                          <Icon name="undo" size={14} /> {t("import.undo")}
                        </ConfirmButton>
                      )}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </Card>
    </>
  );
}

function ImportRowView({
  row,
  included,
  onInclude,
  resolution,
  onResolve,
}: {
  row: Row;
  included: boolean;
  onInclude: (include: boolean) => void;
  resolution: Resolution | undefined;
  onResolve: (resolution: Resolution) => void;
}) {
  const matched = row.status === "matched";
  const changed = (Object.keys(VALUE_KEY) as Field[]).filter(
    (field) => row.values[VALUE_KEY[field]] !== null && row.values[VALUE_KEY[field]] !== undefined,
  );
  const edit = resolution?.kind === "edit" ? resolution.values : null;
  return (
    <tr>
      <td>
        <input
          type="checkbox"
          checked={included}
          disabled={!matched}
          aria-label={t("import.includeNamed", { name: row.filename })}
          onChange={(event) => onInclude(event.target.checked)}
        />
      </td>
      <td>
        <div>{row.filename}</div>
        {matched ? (
          <Link to={`/library/${row.track_id}`} className="muted small">
            {row.path}
          </Link>
        ) : (
          <span className="badge warn">{t(`import.status.${row.status}`)}</span>
        )}
        {row.length_mismatch && (
          <div className="warn small">
            {t("import.lengthMismatch", {
              heron: duration(row.length_ms ?? 0),
              source: duration(row.source_length_ms ?? 0),
            })}
          </div>
        )}
      </td>
      <td>
        <div className="small">
          {changed.map((field) => (
            <span key={field} style={{ marginRight: "0.75rem" }}>
              {fieldLabel(field)} <strong>{show(field, row.values)}</strong>
            </span>
          ))}
        </div>
        {row.warnings.map((warning) => (
          <div key={warning} className="muted small">
            {warning}
          </div>
        ))}
        {matched && row.conflicts.length > 0 && (
          <div className="stack" style={{ marginTop: "0.5rem", gap: "0.4rem" }}>
            <div className="row">
              <span className="badge bad">{t("import.conflict", { count: row.conflicts.length })}</span>
              <Segmented
                label={t("import.conflictFor", { name: row.filename })}
                value={resolution?.kind ?? ("" as "import")}
                onChange={(kind) =>
                  onResolve(
                    kind === "edit"
                      ? {
                          kind,
                          values: Object.fromEntries(
                            row.conflicts.map((field) => [
                              field,
                              TEXT_FIELDS.includes(field)
                                ? String(row.values[VALUE_KEY[field]] ?? "")
                                : String(Number(row.values[VALUE_KEY[field]] ?? 0) / 1000),
                            ]),
                          ),
                        }
                      : { kind },
                  )
                }
                options={[
                  { value: "edit", label: t("import.edit") },
                  { value: "import", label: t("import.useImport") },
                  { value: "heron", label: t("import.keepHeron") },
                ]}
              />
            </div>
            {row.conflicts.map((field) => (
              <div key={field} className="row small">
                <strong>{fieldLabel(field)}</strong>
                <span>
                  {t("import.inHeron")} {show(field, row.current)}
                </span>
                <span>
                  {t("import.inExport")} {show(field, row.values)}
                </span>
                {edit && (
                  <input
                    className="input"
                    style={{ maxWidth: "16rem" }}
                    value={edit[field] ?? ""}
                    inputMode={TEXT_FIELDS.includes(field) ? undefined : "decimal"}
                    aria-label={t("import.editField", { field: fieldLabel(field) })}
                    placeholder={TEXT_FIELDS.includes(field) ? undefined : t("import.seconds")}
                    onChange={(event) =>
                      onResolve({ kind: "edit", values: { ...edit, [field]: event.target.value } })
                    }
                  />
                )}
              </div>
            ))}
          </div>
        )}
      </td>
    </tr>
  );
}
