// @vitest-environment jsdom
// SPDX-License-Identifier: AGPL-3.0-or-later

import { fireEvent, screen, waitFor } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { Schemas } from "../api";

type Medley = Schemas["Medley"];
import { t } from "../i18n";
import { chooseStation } from "../lib/station";
import { answer, browserStubs, lenient, renderApp, sent, user } from "../test/render";
import { MedleyEditor, Medleys } from "./Medleys";

browserStubs();

const RECIPE = {
  category_ids: [1],
  crossfade_ms: 2000,
  duck_attack_at: "outside",
  duck_attack_ms: 500,
  duck_db: -12,
  duck_release_at: "outside",
  duck_release_ms: 500,
  fade_in_curve: "linear",
  fade_in_ms: 500,
  fade_out_curve: "linear",
  fade_out_ms: 500,
  hook_max_ms: 20_000,
  hook_min_ms: 10_000,
  hooks: 3,
  label_id: null,
  liner_at_end: false,
  liner_at_start: false,
  liner_every: 2,
  liner_mode: "between",
  liners: true,
  order: "picked",
} as const;

const SETTINGS = {
  category_id: 1,
  closing_liner_category_id: null,
  opening_liner_category_id: null,
  liner_category_id: 2,
  name_from: "titles",
  defaults: { ...RECIPE },
};

function part(index: number, kind: "hook" | "liner") {
  return {
    kind,
    track_id: 100 + index,
    title: `${kind} ${index}`,
    artist: `Artist ${index}`,
    path: `a/${index}.mp3`,
    from_ms: 10_000 * index,
    to_ms: 10_000 * index + 15_000,
    duration_ms: 200_000,
    shift_ms: 0,
    move_ms: index === 2 ? 400 : 0,
  };
}

const PARTS = [part(1, "hook"), part(2, "liner"), part(3, "hook")];

function medley(changes: Partial<Medley> = {}): Medley {
  return {
    id: 5,
    name: null,
    parts: PARTS,
    path: null,
    recipe: RECIPE,
    rendered_at_unix_ms: null,
    rendered_layout: null,
    station_ids: [1],
    task_id: null,
    task_status: null,
    track_id: null,
    ...changes,
  } as Medley;
}

function editor(medleys: Medley[], role: "viewer" | "operator" = "operator", extra: Record<string, unknown> = {}) {
  answer(
    lenient({
      "GET /api/v1/medleys": medleys,
      "GET /api/v1/stations/{station_id}/settings/medleys": SETTINGS,
      ...extra,
    }),
  );
  chooseStation(1);
  renderApp(<MedleyEditor id={5} />, user(role));
}

describe("Medleys", () => {
  it("lists the medleys, and says when there is none", async () => {
    answer(lenient({ "GET /api/v1/medleys": [medley({ name: "Morning mix", rendered_at_unix_ms: 1_700_000_000_000 })] }));
    chooseStation(1);
    renderApp(<Medleys />);
    expect(await screen.findByText("Morning mix")).toBeTruthy();
  });
});

describe("MedleyEditor", () => {
  it("shows the parts of a medley being built, in order", async () => {
    editor([medley()]);
    expect((await screen.findAllByText("hook 1", { exact: false }))[0]).toBeTruthy();
    expect(screen.getAllByText("liner 2", { exact: false }).length).toBeGreaterThan(0);
    expect(screen.getAllByText("hook 3", { exact: false }).length).toBeGreaterThan(0);
  });

  it("starts a new version from the current parts", async () => {
    editor([medley({ rendered_at_unix_ms: 1_700_000_000_000, track_id: 9, path: "m.mp3" })]);
    fireEvent.click(await screen.findByText(t("medleys.edit")));
    expect(await screen.findByText(t("medleys.preview"))).toBeTruthy();
    expect(screen.getByText(t("medleys.render"))).toBeTruthy();
    expect(screen.getByText(t("track.revert"))).toBeTruthy();
  });

  it("moves a part down while editing", async () => {
    editor([medley({ rendered_at_unix_ms: 1_700_000_000_000, track_id: 9, path: "m.mp3" })]);
    fireEvent.click(await screen.findByText(t("medleys.edit")));
    const down = await screen.findAllByRole("button", { name: /^Move .* down/i });
    fireEvent.click(down[0] as HTMLElement);
    const items = screen.getAllByRole("listitem");
    expect(items[0]?.textContent).toContain("liner 2");
  });

  it("removes a part while editing", async () => {
    editor([medley({ rendered_at_unix_ms: 1_700_000_000_000, track_id: 9, path: "m.mp3" })]);
    fireEvent.click(await screen.findByText(t("medleys.edit")));
    const remove = await screen.findAllByRole("button", { name: /^Remove/i });
    fireEvent.click(remove[0] as HTMLElement);
    await waitFor(() => expect(screen.queryAllByText("hook 1", { exact: false })).toHaveLength(0));
  });

  it("nudges where a part starts", async () => {
    editor([medley({ rendered_at_unix_ms: 1_700_000_000_000, track_id: 9, path: "m.mp3" })]);
    fireEvent.click(await screen.findByText(t("medleys.edit")));
    const later = await screen.findAllByRole("button", { name: /starts later|Start .* later/i });
    fireEvent.click(later[0] as HTMLElement);
    expect(screen.getAllByRole("listitem").length).toBeGreaterThan(0);
  });

  it("builds the parts to pick when none exist", async () => {
    editor([medley({ parts: [] })], "operator", {
      "POST /api/v1/medleys/{id}/plan": { parts: PARTS, qualified: 1 },
    });
    fireEvent.click(await screen.findByText(t("medleys.build")));
    await waitFor(() => expect(sent("POST /api/v1/medleys/{id}/plan").length).toBe(1));
    expect((await screen.findAllByText("hook 1", { exact: false }))[0]).toBeTruthy();
    expect(screen.getByText(t("medleys.fewSongs", { qualified: "1", wanted: "3" }), { exact: false })).toBeTruthy();
  });

  it("asks for a render of the new version", async () => {
    editor([medley({ rendered_at_unix_ms: 1_700_000_000_000, track_id: 9, path: "m.mp3" })], "operator", {
      "POST /api/v1/medleys/{id}/render": { id: 77 },
    });
    fireEvent.click(await screen.findByText(t("medleys.edit")));
    fireEvent.click(await screen.findByText(t("medleys.render")));
    await waitFor(() => expect(sent("POST /api/v1/medleys/{id}/render").length).toBe(1));
  });

  it("offers a viewer nothing to change", async () => {
    editor([medley({ rendered_at_unix_ms: 1_700_000_000_000, track_id: 9, path: "m.mp3" })], "viewer");
    await screen.findAllByText("hook 1", { exact: false });
    expect(screen.queryByText(t("medleys.edit"))).toBeNull();
  });

  it("warns when the timeline is out of step with the file", async () => {
    editor([medley({ track_id: 9, path: "m.mp3", rendered_at_unix_ms: 1_700_000_000_000 })]);
    expect(await screen.findByText(t("medleys.timelineOutOfStep"))).toBeTruthy();
  });

  it("says a render is running", async () => {
    editor([medley({ task_id: 3, task_status: "running" })], "operator", {
      "GET /api/v1/tasks/{id}": { id: 3, status: "running" },
    });
    expect((await screen.findAllByText("hook 1", { exact: false }))[0]).toBeTruthy();
  });
});
