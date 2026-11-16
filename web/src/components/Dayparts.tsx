// SPDX-License-Identifier: AGPL-3.0-or-later

// Daypart separation (docs/rotation.md#daypart-separation): the station's
// division of the day with an example over several days, a category's
// separation and same daypart on the following days, and a pick's daypart
// in the rotation log.

import { useState } from "react";
import type { Explanation } from "../api";
import { rich, t } from "../i18n";
import {
  DAYPART_COUNTS,
  type PartState,
  dayLabel,
  daypartHours,
  daypartLetter,
  daypartsList,
  describeSeparation,
  exampleDays,
  maxSeparation,
} from "../lib/dayparts";
import { dateTime } from "../lib/format";
import { Link } from "../lib/router";
import { Field, Segmented } from "./ui";

const STATES: Record<PartState, "dayparts.played" | "dayparts.kept" | "dayparts.free"> = {
  played: "dayparts.played",
  kept: "dayparts.kept",
  free: "dayparts.free",
};

/**
 * The station's dayparts, with an example over several days. The separation
 * of the example is not saved: each category sets its own.
 */
export function DaypartSettings({
  count,
  disabled,
  onChange,
}: Readonly<{
  count: number;
  disabled: boolean;
  onChange: (count: number) => void;
}>) {
  const [played, setPlayed] = useState(0);
  const [chosenSeparation, setChosenSeparation] = useState(1);
  const max = maxSeparation(count);
  const separation = Math.min(chosenSeparation, max);
  const from = Math.min(played, count - 1);
  const days = exampleDays(count, separation, from);
  return (
    <fieldset className="dayparts">
      <legend>{t("dayparts.title")}</legend>
      <p className="muted small" style={{ marginTop: 0 }}>
        {t("dayparts.help")}
      </p>
      <p className="small">
        {rich("dayparts.perCategory", { link: <Link to="/categories">{t("nav.categories")}</Link> })}{" "}
        {t("dayparts.followingDays")}
      </p>
      <div className="daypart-controls">
        <div className="field">
          <span>{t("dayparts.division")}</span>
          <Segmented
            label={t("dayparts.division")}
            value={String(count)}
            disabled={disabled}
            options={DAYPART_COUNTS.map((n) => ({
              value: String(n),
              label: t("dayparts.divisionOption", { count: n, hours: 24 / n }),
            }))}
            onChange={(value) => onChange(Number(value))}
          />
          <span className="hint">{daypartsList(count)}</span>
        </div>
        <div className="field">
          <span>{t("dayparts.separation")}</span>
          <Segmented
            label={t("dayparts.separation")}
            value={String(separation)}
            options={Array.from({ length: max + 1 }, (_, n) => ({ value: String(n), label: String(n) }))}
            onChange={(value) => setChosenSeparation(Number(value))}
          />
          <span className="hint">{t("dayparts.separationHint", { max, last: count - 1 })}</span>
        </div>
      </div>
      <div className="stack daypart-example">
        <span className="small" style={{ fontWeight: 550 }}>{t("dayparts.example")}</span>
        <div className="daypart-days">
          {days
            .map((states, day) => ({ day, label: dayLabel(day), states }))
            .map(({ day, label, states }) => (
              <DayRow key={label} day={day} count={count} states={states} onPick={day === 0 ? setPlayed : undefined} />
            ))}
          <span />
          <div className="daypart-hours" aria-hidden="true">
            {Array.from({ length: count + 1 }, (_, i) => (i * 24) / count).map((hour) => (
              <span key={hour}>{`${hour}h`}</span>
            ))}
          </div>
        </div>
        <div className="legend small muted">
          {(["played", "kept", "free"] as const).map((state) => (
            <span key={state}>
              <i className={`daypart-dot ${state}`} /> {t(STATES[state])}
            </span>
          ))}
        </div>
        <p className="small" style={{ margin: 0 }}>
          {describeSeparation(count, separation, from)}
        </p>
      </div>
    </fieldset>
  );
}

function DayRow({
  day,
  count,
  states,
  onPick,
}: Readonly<{
  day: number;
  count: number;
  states: PartState[];
  onPick: ((index: number) => void) | undefined;
}>) {
  const label = dayLabel(day);
  return (
    <>
      <span className="small muted">{label}</span>
      <div className="daypart-strip">
        {states.map((state, index) => ({ state, index, letter: daypartLetter(index) })).map(({ state, index, letter }) => {
          const title = `${label} · ${letter}: ${t(STATES[state])}`;
          const content = (
            <>
              <b>{letter}</b>
              <small>{daypartHours(count, index)}</small>
            </>
          );
          return onPick ? (
            <button
              key={letter}
              type="button"
              className={`daypart ${state}`}
              title={title}
              aria-pressed={state === "played"}
              onClick={() => onPick(index)}
            >
              {content}
            </button>
          ) : (
            <span key={letter} className={`daypart ${state}`} title={title}>
              {content}
            </span>
          );
        })}
      </div>
    </>
  );
}

/**
 * A category's daypart separation, 0 (off) by default, described in the
 * station's dayparts. A value over this station's maximum, set for another
 * station's division, stays offered.
 */
export function CategoryDayparts({
  value,
  count,
  disabled,
  onChange,
}: Readonly<{
  value: number;
  /** The station's division of the day. */
  count: number;
  disabled?: boolean;
  onChange: (value: number) => void;
}>) {
  const max = Math.max(maxSeparation(count), value);
  return (
    <Field label={t("dayparts.category")} hint={describeSeparation(count, value)}>
      <select
        className="input"
        value={String(value)}
        disabled={disabled}
        onChange={(event) => onChange(Number(event.target.value))}
      >
        {Array.from({ length: max + 1 }, (_, n) => (
          <option key={n} value={n}>
            {n === 0 ? t("dayparts.offOption") : n}
          </option>
        ))}
      </select>
    </Field>
  );
}

/** The longest the same daypart is kept out on the following days. */
export const MAX_DAYS = 7;

/**
 * A category's same daypart on the following days, 0 (off) to 7 days,
 * with or without its daypart separation.
 */
export function CategoryDaypartDays({
  value,
  disabled,
  onChange,
}: Readonly<{
  value: number;
  disabled?: boolean;
  onChange: (value: number) => void;
}>) {
  return (
    <Field label={t("dayparts.days")} hint={t("dayparts.daysHint")}>
      <select
        className="input"
        value={String(value)}
        disabled={disabled}
        onChange={(event) => onChange(Number(event.target.value))}
      >
        {Array.from({ length: MAX_DAYS + 1 }, (_, n) => (
          <option key={n} value={n}>
            {n === 0 ? t("dayparts.offOption") : t("dayparts.daysOption", { count: n })}
          </option>
        ))}
      </select>
    </Field>
  );
}

/** A pick's daypart: `C`, its hours in the title. */
export function DaypartBadge({ explanation }: Readonly<{ explanation: Explanation }>) {
  const daypart = explanation.daypart;
  if (!daypart) {
    return null;
  }
  const letter = daypartLetter(daypart.daypart);
  return (
    <span
      className="badge daypart-badge"
      title={`${t("dayparts.pickPart", { part: letter })} · ${daypartHours(daypart.count, daypart.daypart)}`}
    >
      {letter}
    </span>
  );
}

/**
 * What daypart separation and the same daypart on the following days did
 * for a pick: where the track last played, how far they were relaxed, how
 * many tracks they held back, and the track's own values when it has
 * them. Nothing when both are off and the category's.
 */
type DaypartLog = NonNullable<Explanation["daypart"]>;

/** The track's own daypart values, for the note. */
function ownValues(daypart: DaypartLog, days: number, ownSeparation: boolean, ownDays: boolean): string {
  const values: string[] = [];
  if (ownSeparation) {
    values.push(`${t("rotation.window.dayparts")} ${daypart.separation}`);
  }
  if (ownDays) {
    values.push(`${t("rotation.window.daypartDays")} ${t("format.days", { count: days })}`);
  }
  return t("trackDayparts.logOwn", { values: values.join(", ") });
}

/** Where the track last played: the separation halved at step 3, else its daypart and time. */
function lastDaypartNote(explanation: Explanation, daypart: DaypartLog): string | null {
  const last = daypart.last_daypart;
  if (last === null || last === undefined) return null;
  if (explanation.step === 3 && daypart.separation > 0) {
    return t("dayparts.halved", {
      from: daypart.separation,
      to: explanation.windows.dayparts,
      played: daypartLetter(last),
      part: daypartLetter(daypart.daypart),
    });
  }
  if (!explanation.last_played_unix_ms) return null;
  return t("dayparts.lastIn", { part: daypartLetter(last), time: dateTime(explanation.last_played_unix_ms) });
}

export function daypartNote(explanation: Explanation): string | null {
  const daypart = explanation.daypart;
  const days = daypart?.days ?? 0;
  const ownSeparation = daypart?.separation_from === "track";
  const ownDays = daypart?.days_from === "track";
  if (
    !daypart ||
    !explanation.enforce_rules ||
    (daypart.separation === 0 && days === 0 && !ownSeparation && !ownDays)
  ) {
    return null;
  }
  const parts: string[] = [];
  if (ownSeparation || ownDays) {
    parts.push(ownValues(daypart, days, ownSeparation, ownDays));
  }
  const step = explanation.step;
  const lastNote = lastDaypartNote(explanation, daypart);
  if (lastNote) {
    parts.push(lastNote);
  }
  if (step === 3 && days > 0) {
    parts.push(t("dayparts.daysHalved", { from: days, to: explanation.windows.daypart_days }));
  }
  const held = explanation.blocked.daypart ?? 0;
  if (held > 0 && step >= 3) {
    parts.push(t("rotationPage.blockedDaypart", { count: held }));
  }
  const heldDays = explanation.blocked.daypart_days ?? 0;
  if (heldDays > 0 && step >= 3) {
    parts.push(t("rotationPage.blockedDaypartDays", { count: heldDays }));
  }
  return parts.length > 0 ? parts.join("; ") : null;
}
