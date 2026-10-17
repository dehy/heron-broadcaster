// SPDX-License-Identifier: AGPL-3.0-or-later

import type { Fade, FadeCurve } from "../api";
import { Field, NumberInput } from "./ui";

export const CURVES: { value: FadeCurve; label: string }[] = [
  { value: "linear", label: "Linear" },
  { value: "s_curve", label: "S-curve" },
  { value: "logarithmic", label: "Logarithmic (holds, then drops)" },
  { value: "exponential", label: "Exponential (drops, then trails)" },
];

/** One side of a crossfade: on or off, its curve, length and level. */
export function FadeEditor({
  value,
  onChange,
  kind,
  disabled,
}: {
  value: Fade;
  onChange: (fade: Fade) => void;
  kind: "in" | "out";
  disabled?: boolean;
}) {
  return (
    <div className="stack">
      <label className="check">
        <input
          type="checkbox"
          checked={value.enabled}
          disabled={disabled}
          onChange={(event) => onChange({ ...value, enabled: event.target.checked })}
        />
        {kind === "out" ? "Fade the outgoing track out" : "Fade the incoming track in"}
      </label>
      <div className="form-grid">
        <Field label="Curve">
          <select
            className="input"
            value={value.curve}
            disabled={disabled || !value.enabled}
            onChange={(event) => onChange({ ...value, curve: event.target.value as FadeCurve })}
          >
            {CURVES.map((curve) => (
              <option key={curve.value} value={curve.value}>
                {curve.label}
              </option>
            ))}
          </select>
        </Field>
        <Field label="Length" hint={kind === "out" ? "From when the next track starts" : "From the cue-in"}>
          <NumberInput
            value={value.time_ms / 1000}
            min={0}
            max={30}
            step={0.1}
            suffix="s"
            disabled={disabled || !value.enabled}
            onChange={(seconds) => onChange({ ...value, time_ms: Math.round(seconds * 1000) })}
          />
        </Field>
        <Field label={kind === "out" ? "Down to" : "Up from"} hint="0 % is silence">
          <NumberInput
            value={value.level_pct}
            min={0}
            max={100}
            step={1}
            suffix="%"
            disabled={disabled || !value.enabled}
            onChange={(level) => onChange({ ...value, level_pct: Math.round(level) })}
          />
        </Field>
      </div>
    </div>
  );
}
