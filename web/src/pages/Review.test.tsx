// @vitest-environment jsdom
// SPDX-License-Identifier: AGPL-3.0-or-later

import { fireEvent, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { t } from "../i18n";
import { chooseStation } from "../lib/station";
import { answer, browserStubs, lenient, renderApp, sent, user } from "../test/render";
import { Review } from "./Review";

browserStubs();

afterEach(() => window.history.pushState({}, "", "/"));

const BATCHES = [
  { id: 1, kind: "import", files: 4, created_at_unix_ms: 1_700_000_000_000, finished_at_unix_ms: 1_700_000_100_000 },
  { id: 2, kind: "organize", files: 2, created_at_unix_ms: 1_700_000_200_000, finished_at_unix_ms: null },
  { id: 3, kind: "edit", files: 1, created_at_unix_ms: 1_700_000_300_000, finished_at_unix_ms: 1_700_000_310_000, undone_by: 4 },
  { id: 4, kind: "undo", files: 1, undoes: 3, created_at_unix_ms: 1_700_000_400_000, finished_at_unix_ms: 1_700_000_410_000 },
];

function batches(task: unknown) {
  window.history.pushState({}, "", "/?tab=batches");
  answer(
    lenient({
      "GET /api/v1/library/batches": BATCHES,
      "POST /api/v1/library/batches/{id}/undo": { id: 9 },
      "GET /api/v1/library/tasks/{id}": task,
    }),
  );
  chooseStation(1);
  renderApp(<Review />, user("admin"));
}

describe("Review changes", () => {
  it("lists the batches with how each ended", async () => {
    batches({ id: 9, status: "running" });
    expect((await screen.findAllByText(t("review.done"))).length).toBeGreaterThan(0);
    expect(screen.getByText(t("review.unfinished"))).toBeTruthy();
    expect(screen.getByText(t("review.undoneBy", { batch: "4" }))).toBeTruthy();
    expect(screen.getByText(t("review.of", { batch: "3" }), { exact: false })).toBeTruthy();
  });

  it.each([
    ["no report", { id: 9, status: "done" }],
    ["everything undone", { id: 9, status: "done", report: { done: 4, skipped: [] } }],
    ["some skipped", { id: 9, status: "done", report: { done: 3, skipped: [{ reason: "moved" }] } }],
  ])("undoes a batch (%s)", async (_name, task) => {
    batches(task);
    const undo = () => screen.findAllByRole("button", { name: t("review.kind.undo") });
    fireEvent.click((await undo())[0] as HTMLElement);
    fireEvent.click((await undo())[1] as HTMLElement);
    await waitFor(() => expect(sent("POST /api/v1/library/batches/{id}/undo")).toHaveLength(1));
    await waitFor(() => expect(sent("GET /api/v1/library/tasks/{id}").length).toBeGreaterThan(0));
  });
});
