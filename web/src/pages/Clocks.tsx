// SPDX-License-Identifier: AGPL-3.0-or-later

// Clocks: ordered slots, each calling a category. The editor shows the
// hour as it will sound (expected starts, categories running short) and
// simulates it.

import {
  DndContext,
  type DragEndEvent,
  KeyboardSensor,
  PointerSensor,
  closestCenter,
  useSensor,
  useSensors,
} from "@dnd-kit/core";
import {
  SortableContext,
  arrayMove,
  sortableKeyboardCoordinates,
  useSortable,
  verticalListSortingStrategy,
} from "@dnd-kit/sortable";
import { CSS } from "@dnd-kit/utilities";
import { keepPreviousData, useQuery } from "@tanstack/react-query";
import { useEffect, useMemo, useRef, useState } from "react";
import {
  type Category,
  type ClockPreview,
  type ClockSlot,
  type ClockView,
  type Method,
  type SimulationReport,
  api,
  done,
  unwrap,
} from "../api";
import { SeparationEditor, applyOverrides } from "../components/Separation";
import { useDebounced } from "../components/TrackSearch";
import {
  Banner,
  Card,
  ConfirmButton,
  Empty,
  ErrorView,
  Field,
  Icon,
  Loading,
  Modal,
  PageHead,
} from "../components/ui";
import { count, duration } from "../lib/format";
import { METHOD_OPTIONS, describeOverrides } from "../lib/rotation";
import { Link, navigate } from "../lib/router";
import { useAction, useCan } from "../lib/session";

const HOUR_MS = 3_600_000;

export function useCategories() {
  return useQuery({
    queryKey: ["categories"],
    queryFn: () => unwrap(api.GET("/api/v1/categories")),
  });
}

export function Clocks() {
  const canEdit = useCan("operator");
  const clocks = useQuery({ queryKey: ["clocks"], queryFn: () => unwrap(api.GET("/api/v1/clocks")) });
  const categories = useCategories();
  const byId = new Map((categories.data ?? []).map((c) => [c.id, c]));
  return (
    <>
      <PageHead
        title="Clocks"
        description={
          <>
            A clock is an hour of programming: slots in order, each calling a category. The{" "}
            <Link to="/schedule">schedule</Link> says which clock runs when.
          </>
        }
        actions={
          canEdit && (
            <button type="button" className="btn primary" onClick={() => navigate("/clocks/new")}>
              <Icon name="plus" /> New clock
            </button>
          )
        }
      />
      <Card flush>
        {clocks.isPending && <Loading />}
        {clocks.isError && <ErrorView error={clocks.error} />}
        {clocks.data?.length === 0 && (
          <Empty>No clock yet: every hour rotates the whole library, least recently played first.</Empty>
        )}
        {clocks.data && clocks.data.length > 0 && (
          <div className="table-wrap">
            <table className="table">
              <thead>
                <tr>
                  <th>Name</th>
                  <th>Slots</th>
                  <th>Scheduled</th>
                </tr>
              </thead>
              <tbody>
                {clocks.data.map((clock) => (
                  <tr key={clock.id}>
                    <td>
                      <Link to={`/clocks/${clock.id}`}>{clock.name}</Link>
                      {clock.looping && (
                        <>
                          {" "}
                          <span className="badge" title="Carries on across hours">
                            Loops
                          </span>
                        </>
                      )}
                    </td>
                    <td>
                      <span className="slot-strip" title={`${clock.slots.length} slots`}>
                        {clock.slots.map((slot, index) => (
                          <span
                            key={index}
                            className="swatch"
                            title={byId.get(slot.category_id)?.name}
                            style={{ background: byId.get(slot.category_id)?.color ?? "var(--wave)" }}
                          />
                        ))}
                        {clock.slots.length === 0 && <span className="muted small">empty</span>}
                      </span>
                    </td>
                    <td className="num">
                      {clock.scheduled_hours === 0 ? (
                        <span className="muted">not scheduled</span>
                      ) : (
                        count(clock.scheduled_hours, "hour") + " a week"
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

/** A slot as edited, with a key that stays when slots move. */
interface Draft extends ClockSlot {
  key: number;
}

export function ClockEditor({ id }: { id: number | null }) {
  const clock = useQuery({
    queryKey: ["clocks", id],
    queryFn: () => unwrap(api.GET("/api/v1/clocks/{id}", { params: { path: { id: id ?? 0 } } })),
    enabled: id !== null,
  });
  if (id !== null && clock.isPending) {
    return <Loading />;
  }
  if (clock.isError) {
    return <ErrorView error={clock.error} />;
  }
  return <Editor key={id ?? "new"} clock={clock.data ?? null} />;
}

function Editor({ clock }: { clock: ClockView | null }) {
  const canEdit = useCan("operator");
  const categories = useCategories();
  const rotation = useQuery({
    queryKey: ["settings", "rotation"],
    queryFn: () => unwrap(api.GET("/api/v1/settings/rotation")),
  });
  const nextKey = useRef(1);
  const withKeys = (slots: ClockSlot[]): Draft[] => slots.map((slot) => ({ ...slot, key: nextKey.current++ }));
  const [name, setName] = useState(clock?.name ?? "");
  const [looping, setLooping] = useState(clock?.looping ?? false);
  const [slots, setSlots] = useState<Draft[]>(() => withKeys(clock?.slots ?? []));
  const [separating, setSeparating] = useState<number | null>(null);
  const saved = useMemo(
    () => JSON.stringify({ name: clock?.name ?? "", looping: clock?.looping ?? false, slots: clock?.slots ?? [] }),
    [clock],
  );
  const plain = slots.map(({ key: _key, ...slot }) => slot);
  const dirty = JSON.stringify({ name, looping, slots: plain }) !== saved;

  const settled = useDebounced(plain, 300);
  const preview = useQuery({
    queryKey: ["clock-preview", settled, looping],
    queryFn: () => unwrap(api.POST("/api/v1/clocks/preview", { body: { slots: settled, looping } })),
    placeholderData: keepPreviousData,
  });

  const save = useAction(
    () => {
      const body = { name, looping, slots: plain };
      return clock
        ? unwrap(api.PUT("/api/v1/clocks/{id}", { params: { path: { id: clock.id } }, body }))
        : unwrap(api.POST("/api/v1/clocks", { body }));
    },
    {
      success: "Clock saved",
      invalidate: [["clocks"]],
      onSuccess: (saved) => {
        if (!clock) {
          navigate(`/clocks/${saved.id}`, { replace: true });
        }
      },
    },
  );
  const remove = useAction(
    () => done(api.DELETE("/api/v1/clocks/{id}", { params: { path: { id: clock?.id ?? 0 } } })),
    { success: "Clock deleted", invalidate: [["clocks"], ["schedule"]], onSuccess: () => navigate("/clocks") },
  );

  const sensors = useSensors(
    useSensor(PointerSensor, { activationConstraint: { distance: 4 } }),
    useSensor(KeyboardSensor, { coordinateGetter: sortableKeyboardCoordinates }),
  );
  const onDragEnd = (event: DragEndEvent) => {
    const { active, over } = event;
    if (!over || active.id === over.id) {
      return;
    }
    const from = slots.findIndex((slot) => slot.key === active.id);
    const to = slots.findIndex((slot) => slot.key === over.id);
    if (from >= 0 && to >= 0) {
      setSlots(arrayMove(slots, from, to));
    }
  };
  const update = (key: number, changes: Partial<ClockSlot>) =>
    setSlots((current) => current.map((slot) => (slot.key === key ? { ...slot, ...changes } : slot)));

  const byId = new Map((categories.data ?? []).map((c) => [c.id, c]));
  const firstCategory = categories.data?.[0];
  const editingSlot = slots.find((slot) => slot.key === separating);
  const shown = preview.data && preview.data.slots.length === slots.length ? preview.data : undefined;

  return (
    <>
      <PageHead
        title={clock ? clock.name : "New clock"}
        description={<Link to="/clocks">All clocks</Link>}
        actions={
          canEdit && (
            <>
              {clock && (
                <ConfirmButton
                  className="btn danger"
                  confirm="Delete"
                  question={
                    <p>
                      Delete <strong>{clock.name}</strong>? The hours it is scheduled at rotate the whole library
                      until another clock is scheduled.
                    </p>
                  }
                  onConfirm={() => remove.mutate(undefined)}
                >
                  <Icon name="trash" /> Delete
                </ConfirmButton>
              )}
              <button
                type="button"
                className="btn primary"
                disabled={!dirty || !name.trim() || save.isPending}
                onClick={() => save.mutate(undefined)}
              >
                Save
              </button>
            </>
          )
        }
      />
      <div className="grid sidebar-right">
        <div className="stack">
          <Card title="Slots" flush>
            <div className="card-pad">
              <Field label="Name">
                <input
                  className="input"
                  value={name}
                  maxLength={255}
                  disabled={!canEdit}
                  onChange={(event) => setName(event.target.value)}
                />
              </Field>
              <label className="check" style={{ marginTop: "0.75rem" }}>
                <input
                  type="checkbox"
                  checked={looping}
                  disabled={!canEdit}
                  onChange={(event) => setLooping(event.target.checked)}
                />
                Loop across hours
              </label>
              <p className="muted small" style={{ margin: "0.25rem 0 0" }}>
                {looping
                  ? "The clock carries on from its last slot whatever the hour, like a SAM Broadcaster clockwheel: it can be longer than an hour."
                  : "Each hour starts the clock from its first slot; slots that do not fit in the hour are dropped."}
              </p>
            </div>
            {categories.data?.length === 0 && (
              <Banner kind="warn">
                Slots call categories: <Link to="/categories">create some</Link> first.
              </Banner>
            )}
            {slots.length === 0 ? (
              <Empty>No slot yet.</Empty>
            ) : (
              <DndContext sensors={sensors} collisionDetection={closestCenter} onDragEnd={onDragEnd}>
                <SortableContext items={slots.map((slot) => slot.key)} strategy={verticalListSortingStrategy}>
                  <ol className="list" aria-label="Slots">
                    {slots.map((slot, index) => (
                      <SlotRow
                        key={slot.key}
                        slot={slot}
                        index={index}
                        categories={categories.data ?? []}
                        preview={shown?.slots[index]}
                        canEdit={canEdit}
                        onChange={(changes) => update(slot.key, changes)}
                        onSeparation={() => setSeparating(slot.key)}
                        onRemove={() => setSlots(slots.filter((s) => s.key !== slot.key))}
                      />
                    ))}
                  </ol>
                </SortableContext>
              </DndContext>
            )}
            {canEdit && firstCategory && (
              <div className="card-pad row">
                <button
                  type="button"
                  className="btn"
                  onClick={() => {
                    const last = slots[slots.length - 1];
                    setSlots([
                      ...slots,
                      {
                        key: nextKey.current++,
                        category_id: last?.category_id ?? firstCategory.id,
                        method: "least_recently_played",
                        enforce_rules: true,
                        separation: {},
                      },
                    ]);
                  }}
                >
                  <Icon name="plus" /> Add a slot
                </button>
                <span className="muted small">Drag the slots to reorder them.</span>
              </div>
            )}
          </Card>
        </div>
        <div className="stack">
          <HourCard preview={shown} categories={byId} slots={plain} looping={looping} />
          {clock && <SimulateCard clockId={clock.id} dirty={dirty} />}
        </div>
      </div>
      {editingSlot && (
        <Modal
          title={`Slot ${slots.indexOf(editingSlot) + 1}: separation`}
          open
          onClose={() => setSeparating(null)}
          footer={
            <button type="button" className="btn primary" onClick={() => setSeparating(null)}>
              Done
            </button>
          }
        >
          <p className="muted small">
            Override the windows of {byId.get(editingSlot.category_id)?.name ?? "the category"} for this slot only.
          </p>
          <SeparationEditor
            value={editingSlot.separation ?? {}}
            inherited={
              rotation.data
                ? applyOverrides(rotation.data.separation, byId.get(editingSlot.category_id)?.separation)
                : undefined
            }
            disabled={!canEdit}
            onChange={(separation) => update(editingSlot.key, { separation })}
          />
        </Modal>
      )}
    </>
  );
}

function SlotRow({
  slot,
  index,
  categories,
  preview,
  canEdit,
  onChange,
  onSeparation,
  onRemove,
}: {
  slot: Draft;
  index: number;
  categories: Category[];
  preview: ClockPreview["slots"][number] | undefined;
  canEdit: boolean;
  onChange: (changes: Partial<ClockSlot>) => void;
  onSeparation: () => void;
  onRemove: () => void;
}) {
  const { attributes, listeners, setNodeRef, transform, transition, isDragging } = useSortable({
    id: slot.key,
    disabled: !canEdit,
  });
  const category = categories.find((c) => c.id === slot.category_id);
  const overrides = describeOverrides(slot.separation);
  return (
    <li
      ref={setNodeRef}
      className={`queue-item slot${isDragging ? " dragging" : ""}${preview?.beyond_hour ? " beyond" : ""}`}
      style={{ transform: CSS.Transform.toString(transform), transition }}
    >
      {canEdit && (
        <button
          type="button"
          className="btn ghost small icon handle"
          aria-label={`Move slot ${index + 1}`}
          {...attributes}
          {...listeners}
        >
          <Icon name="grip" />
        </button>
      )}
      <span className="index">{index + 1}</span>
      <span className="swatch" style={{ background: category?.color ?? "var(--wave)" }} />
      <div className="meta stack" style={{ gap: "0.35rem" }}>
        <div className="row">
          <select
            className="input"
            aria-label={`Category of slot ${index + 1}`}
            value={slot.category_id}
            disabled={!canEdit}
            onChange={(event) => onChange({ category_id: Number(event.target.value) })}
          >
            {categories.map((c) => (
              <option key={c.id} value={c.id}>
                {c.name}
              </option>
            ))}
          </select>
          <select
            className="input"
            aria-label={`Selection method of slot ${index + 1}`}
            value={slot.method}
            disabled={!canEdit}
            onChange={(event) => onChange({ method: event.target.value as Method })}
          >
            {METHOD_OPTIONS.map((option) => (
              <option key={option.value} value={option.value}>
                {option.label}
              </option>
            ))}
          </select>
          <label className="check small" title="Off: separation rules are skipped (restrictions still apply)">
            <input
              type="checkbox"
              checked={slot.enforce_rules}
              disabled={!canEdit}
              onChange={(event) => onChange({ enforce_rules: event.target.checked })}
            />
            Enforce rules
          </label>
        </div>
        <div className="row small muted" style={{ gap: "0.4rem" }}>
          {preview && (
            <>
              <span className="num" title="Expected start, from the top of the hour">
                +{duration(preview.expected_start_ms)}
              </span>
              <span>·</span>
              <span>
                {count(preview.tracks, "track")}
                {preview.average_ms !== null && preview.average_ms !== undefined
                  ? `, ${duration(preview.average_ms)} on average`
                  : ""}
              </span>
              {preview.short && (
                <span
                  className="badge warn"
                  title={`Needs about ${preview.needed_tracks} tracks not to repeat within its track window`}
                >
                  Running short
                </span>
              )}
              {preview.beyond_hour && <span className="badge">After the hour: not played</span>}
            </>
          )}
          <button type="button" className="link-button" onClick={onSeparation}>
            {overrides ? `Separation: ${overrides}` : canEdit ? "Override separation" : "Separation"}
          </button>
        </div>
      </div>
      {canEdit && (
        <button
          type="button"
          className="btn ghost small icon danger"
          aria-label={`Remove slot ${index + 1}`}
          onClick={onRemove}
        >
          <Icon name="trash" />
        </button>
      )}
    </li>
  );
}

/** Marks along the bar: quarters of the hour, or half hours of a long loop. */
function ticks(span: number): { at: number; label: string }[] {
  if (span <= HOUR_MS) {
    return [0, 15, 30, 45, 60].map((minute) => ({ at: minute * 60_000, label: `:${String(minute).padStart(2, "0")}` }));
  }
  const marks = [];
  for (let at = 0; at <= span; at += 30 * 60_000) {
    marks.push({ at, label: duration(at).replace(/:00$/, "") });
  }
  return marks;
}

/** The hour (or the loop), drawn: each slot as long as its category's average track. */
function HourCard({
  preview,
  categories,
  slots,
  looping,
}: {
  preview: ClockPreview | undefined;
  categories: Map<number, Category>;
  slots: ClockSlot[];
  looping: boolean;
}) {
  const short = preview?.slots.filter((slot) => slot.short) ?? [];
  const span = looping && preview ? Math.max(HOUR_MS, preview.duration_ms) : HOUR_MS;
  return (
    <Card title={looping ? "The loop" : "The hour"}>
      {!preview ? (
        <Loading />
      ) : slots.length === 0 ? (
        <Empty>Add slots to see the hour.</Empty>
      ) : (
        <div className="stack">
          <div className="hour-bar" role="img" aria-label={looping ? "Slots over one round of the loop" : "Slots over the hour"}>
            {preview.slots.map((slot, index) => {
              const start = slot.expected_start_ms;
              if (start >= span) {
                return null;
              }
              const length = Math.min(slot.average_ms ?? 210_000, span - start);
              return (
                <span
                  key={index}
                  className="segment"
                  title={`${index + 1}. ${categories.get(slot.category_id)?.name ?? "?"} at +${duration(start)}`}
                  style={{
                    left: `${(start / span) * 100}%`,
                    width: `${(length / span) * 100}%`,
                    background: slot.color ?? "var(--wave)",
                  }}
                />
              );
            })}
            {preview.duration_ms > 0 && preview.duration_ms < span && (
              <span
                className="segment repeat"
                title="The clock starts over"
                style={{ left: `${(preview.duration_ms / span) * 100}%`, right: 0 }}
              />
            )}
          </div>
          <div className="hour-ticks muted small" aria-hidden="true">
            {ticks(span).map((tick) => (
              <span key={tick.at}>{tick.label}</span>
            ))}
          </div>
          <dl className="facts">
            <dt>{looping ? "One round" : "Expected length"}</dt>
            <dd className="num">{duration(preview.duration_ms)}</dd>
            {preview.runs_per_hour > 1 && (
              <>
                <dt>Starts over</dt>
                <dd>{preview.runs_per_hour.toFixed(1)} times an hour</dd>
              </>
            )}
            {!looping && preview.duration_ms > HOUR_MS && (
              <>
                <dt>Past the hour</dt>
                <dd>
                  {count(preview.slots.filter((s) => s.beyond_hour).length, "slot")} will not play: the next hour
                  starts its own clock. Make the clock loop across hours to play them all.
                </dd>
              </>
            )}
          </dl>
          {short.length > 0 && (
            <Banner kind="warn">
              {[...new Set(short.map((slot) => categories.get(slot.category_id)?.name ?? "A category"))].join(", ")}{" "}
              {short.length === 1 ? "has" : "have"} too few tracks for {short.length === 1 ? "its" : "their"} track
              separation: expect relaxed rules.
            </Banner>
          )}
          <p className="muted small" style={{ margin: 0 }}>
            Lengths are the categories' averages; real tracks vary.{" "}
            {looping
              ? "The loop carries on across hours, from where it stopped."
              : "Each new hour starts its clock from the first slot."}
          </p>
        </div>
      )}
    </Card>
  );
}

function SimulateCard({ clockId, dirty }: { clockId: number; dirty: boolean }) {
  const canSimulate = useCan("operator");
  const [report, setReport] = useState<SimulationReport | null>(null);
  const simulate = useAction(
    () => unwrap(api.POST("/api/v1/rotation/simulate", { body: { hours: 1, clock_id: clockId } })),
    { onSuccess: (result) => setReport(result) },
  );
  useEffect(() => setReport(null), [clockId]);
  if (!canSimulate) {
    return null;
  }
  const names = new Map((report?.tracks ?? []).map((track) => [track.id, track]));
  const start = report?.simulation.start_unix_ms ?? 0;
  return (
    <Card
      title="Simulation"
      actions={
        <button
          type="button"
          className="btn small"
          disabled={dirty || simulate.isPending}
          title={dirty ? "Save the clock first" : undefined}
          onClick={() => simulate.mutate(undefined)}
        >
          {simulate.isPending ? "Simulating…" : "Simulate an hour"}
        </button>
      }
    >
      {!report ? (
        <p className="muted small" style={{ margin: 0 }}>
          Picks an hour of this clock from the library and the history, without touching the queue.
        </p>
      ) : (
        <div className="stack">
          {report.simulation.violations.length > 0 && (
            <Banner kind="bad">{count(report.simulation.violations.length, "rule")} broken: please report it.</Banner>
          )}
          <ol className="list compact">
            {report.simulation.picks.map((pick, index) => {
              const track = names.get(pick.track_id);
              return (
                <li key={index} className="row" style={{ flexWrap: "nowrap" }}>
                  <span className="num muted small">+{duration(pick.start_unix_ms - start)}</span>
                  <span className="ellipsis" style={{ flex: 1 }}>
                    <Link to={`/library/${pick.track_id}`}>
                      {track ? `${track.artist ?? "?"} – ${track.title ?? "?"}` : `Track ${pick.track_id}`}
                    </Link>
                  </span>
                  {pick.explanation.step > 0 && (
                    <span className="badge warn" title={pick.explanation.step_description}>
                      Relaxed {pick.explanation.step}
                    </span>
                  )}
                </li>
              );
            })}
          </ol>
          <p className="muted small" style={{ margin: 0 }}>
            Random and weighted slots pick differently each time.
          </p>
        </div>
      )}
    </Card>
  );
}
