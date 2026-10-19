// SPDX-License-Identifier: AGPL-3.0-or-later

// Words for the codes the API sends: why a track plays, roles, states.

import type { Role } from "../api";
import { type PlainKey, t } from "../i18n";

const SOURCES: Record<string, PlainKey> = {
  rotation: "source.rotation",
  request: "source.request",
  fallback: "source.fallback",
  manual: "source.manual",
};

/** Why a track is queued or was played; a code the web does not know is shown as it is. */
export function sourceLabel(source: string): string {
  const key = SOURCES[source];
  return key ? t(key) : source;
}

const ENDINGS: Record<string, PlainKey> = {
  finished: "history.finished",
  skipped: "history.skipped",
  stopped: "history.stopped",
  replaced: "history.replaced",
};

/** How a play ended: `Played`, `Skipped`…; `Ended` when the engine did not say. */
export function endingLabel(reason: string | null | undefined): string {
  const key = reason ? ENDINGS[reason] : undefined;
  return key ? t(key) : (reason ?? t("history.ended"));
}

export function roleLabel(role: Role): string {
  return t(`role.${role}`);
}
