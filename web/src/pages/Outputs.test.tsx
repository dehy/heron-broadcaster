// @vitest-environment jsdom
// SPDX-License-Identifier: AGPL-3.0-or-later

import { fireEvent, screen, waitFor } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { t } from "../i18n";
import { chooseStation } from "../lib/station";
import { answer, browserStubs, lenient, renderApp, sent, user } from "../test/render";
import { Outputs } from "./Outputs";

browserStubs();

const profile = (id: number, name: string, encoder: string, options: Record<string, unknown>, bitrate: number | null) => ({
  id,
  name,
  encoder,
  options,
  bitrate_kbps: bitrate,
  channels: 2,
  sample_rate: 44_100,
  outputs: id === 1 ? 1 : 0,
});

const PROFILES = [
  profile(1, "Stream MP3", "mp3", { mode: "cbr", quality: 2 }, 128),
  profile(2, "Archive", "external_ffmpeg", { codec: "libopus", profile: "voip" }, null),
  profile(3, "Mobile", "aac", {}, 64),
];

function page() {
  answer(
    lenient({
      "GET /api/v1/encoder-profiles": PROFILES,
      "PUT /api/v1/encoder-profiles/{id}": undefined,
    }),
  );
  chooseStation(1);
  renderApp(<Outputs />, user("admin"));
}

describe("Outputs", () => {
  it("describes each encoder profile", async () => {
    page();
    expect(await screen.findByText("Stream MP3")).toBeTruthy();
    expect(screen.getByText("Archive")).toBeTruthy();
    expect(screen.getByText(/CBR/)).toBeTruthy();
    expect(screen.getByText(/libopus voip/)).toBeTruthy();
  });

  it.each(["Stream MP3", "Archive", "Mobile"])("opens the %s profile to edit it", async (name) => {
    page();
    fireEvent.click(await screen.findByLabelText(t("categories.edit", { name })));
    expect((await screen.findAllByDisplayValue(name)).length).toBeGreaterThan(0);
  });

  it("saves an edited profile", async () => {
    page();
    fireEvent.click(await screen.findByLabelText(t("categories.edit", { name: "Stream MP3" })));
    await screen.findAllByDisplayValue("Stream MP3");
    fireEvent.click(screen.getByText(t("common.save")));
    await waitFor(() => expect(sent("PUT /api/v1/encoder-profiles/{id}")).toHaveLength(1));
  });
});
