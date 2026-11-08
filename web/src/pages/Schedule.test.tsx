// @vitest-environment jsdom
// SPDX-License-Identifier: AGPL-3.0-or-later

import { fireEvent, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { t } from "../i18n";
import { chooseStation } from "../lib/station";
import { answer, browserStubs, lenient, renderApp, user } from "../test/render";
import { Schedule } from "./Schedule";

browserStubs();

const SCHEDULE = {
  id: 1,
  name: "Week",
  is_default: true,
  periods: [{ start: "12-24", end: "12-26", every_year: true }],
  hours: [
    { day_of_week: 1, hour: 8, clock_id: 3 },
    { day_of_week: 1, hour: 9, clock_id: 3 },
  ],
};

function page(role: "viewer" | "operator" = "operator", clocks: unknown[] = [{ id: 3, name: "Morning", slots: [] }]) {
  answer(
    lenient({
      "GET /api/v1/stations/{station_id}/clocks": clocks,
      "GET /api/v1/stations/{station_id}/schedules": {
        active_id: 1,
        next_change: null,
        schedules: [{ id: 1, name: "Week", is_default: true, hours: 2, periods: [] }],
      },
      "GET /api/v1/stations/{station_id}/schedules/{id}": SCHEDULE,
      "GET /api/v1/stations/{station_id}/settings/station": { timezone: "Europe/Paris", week_starts_on: 1 },
    }),
  );
  chooseStation(1);
  renderApp(<Schedule />, user(role));
}

describe("Schedule", () => {
  it("shows the clocks to paint with and the schedules", async () => {
    page();
    expect((await screen.findAllByText("Morning")).length).toBeGreaterThan(0);
    expect(screen.getAllByText(t("schedules.default")).length).toBeGreaterThan(0);
  });

  it("paints a whole day, which can then be saved or reverted", async () => {
    page();
    await screen.findAllByText("Morning");
    fireEvent.click(screen.getAllByRole("button", { name: /^Paint (?!\d)/ })[0] as HTMLElement);
    expect(screen.getByRole("button", { name: t("common.save") })).toBeTruthy();
  });

  it("paints an hour of every day", async () => {
    page();
    await screen.findAllByText("Morning");
    fireEvent.click(screen.getAllByRole("button", { name: /^Paint/ })[0] as HTMLElement);
    expect(screen.getByRole("button", { name: t("common.save") })).toBeTruthy();
  });

  it("does not offer painting to a viewer", async () => {
    page("viewer");
    await screen.findAllByText(t("schedules.default"));
    expect(screen.queryByText(t("schedule.paintWith"))).toBeNull();
  });

  it("sends to the clocks when none exists", async () => {
    page("operator", []);
    expect(await screen.findByText(t("schedule.createOne"))).toBeTruthy();
  });
});
