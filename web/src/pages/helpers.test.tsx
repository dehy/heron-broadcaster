// @vitest-environment jsdom
// SPDX-License-Identifier: AGPL-3.0-or-later

import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { t } from "../i18n";
import { matchesType, sortDirection } from "./Categories";
import { separationLabel } from "./Clocks";
import { show } from "./Import";
import { previewIcon } from "./Library";
import { EngineVariable } from "./Stations";

describe("categories", () => {
  const category = (type_id: number | null | undefined) => ({ type_id }) as never;

  it("matches every category, the untyped ones, or one type", () => {
    expect(matchesType(category(3), "all")).toBe(true);
    expect(matchesType(category(null), "none")).toBe(true);
    expect(matchesType(category(undefined), "none")).toBe(true);
    expect(matchesType(category(3), "none")).toBe(false);
    expect(matchesType(category(3), 3)).toBe(true);
    expect(matchesType(category(4), 3)).toBe(false);
  });

  it("says which way a column is sorted", () => {
    expect(sortDirection({ by: "name", descending: false }, "name")).toBe("ascending");
    expect(sortDirection({ by: "name", descending: true }, "name")).toBe("descending");
    expect(sortDirection({ by: "name", descending: true }, "type" as never)).toBeUndefined();
  });
});

describe("clocks", () => {
  it("labels the separation of a slot", () => {
    expect(separationLabel("3, 4", true)).toBe(t("clocks.separationValue", { windows: "3, 4" }));
    expect(separationLabel("", true)).toBe(t("clocks.overrideSeparation"));
    expect(separationLabel("", false)).toBe(t("categories.separation"));
  });
});

describe("library", () => {
  it("shows a stop icon while previewing", () => {
    expect(previewIcon(true)).toBe("stop");
    expect(previewIcon(false)).toBe("play");
  });
});

describe("import", () => {
  it("shows text as it is, a time as a duration, and nothing as a dash", () => {
    expect(show("title", { title: "Blue Monday" } as never)).toBe("Blue Monday");
    expect(show("title", {} as never)).toBe("–");
    expect(show("title", { title: null } as never)).toBe("–");
    expect(show("intro", { intro_ms: 61_000 } as never)).not.toBe("–");
  });
});

describe("stations", () => {
  const station = (changes: Record<string, unknown>) =>
    ({ engine_variable: "HERON_ENGINE_A", engine_taken_by: null, engine_configured: false, ...changes }) as never;

  it("says which station an engine is taken by", () => {
    render(<EngineVariable station={station({ engine_taken_by: "fm" })} />);
    expect(screen.getByText(t("stations.engineTaken", { code: "fm" }))).toBeTruthy();
  });

  it("says an engine is configured", () => {
    render(<EngineVariable station={station({ engine_configured: true })} />);
    expect(screen.getByText(t("stations.engineConfigured"))).toBeTruthy();
  });
});
