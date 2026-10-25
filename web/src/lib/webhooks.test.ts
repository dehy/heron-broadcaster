// SPDX-License-Identifier: AGPL-3.0-or-later

import { describe, expect, it } from "vitest";
import { DEFAULT_TEMPLATE, eventLabel, toggleEvent, webhookBody } from "./webhooks";

describe("events", () => {
  it("have labels", () => {
    expect(eventLabel("track.started")).toBe("Track started");
    expect(eventLabel("track.skipped")).toBe("Track skipped");
    expect(eventLabel("ping")).toBe("Test");
    expect(eventLabel("track.paused")).toBe("track.paused");
  });

  it("stay in order when checked", () => {
    expect(toggleEvent(["track.skipped"], "track.started", true)).toEqual(["track.started", "track.skipped"]);
    expect(toggleEvent(["track.started", "track.ended"], "track.started", false)).toEqual(["track.ended"]);
    expect(toggleEvent(["track.ended"], "track.ended", true)).toEqual(["track.ended"]);
  });
});

describe("the saved body", () => {
  const form = {
    name: "Website",
    url: "https://example.org/heron",
    events: ["track.started" as const],
    includeFx: true,
    enabled: false,
    custom: false,
    contentType: "text/plain",
    template: DEFAULT_TEMPLATE,
  };

  it("is the default JSON body unless a template is chosen", () => {
    expect(webhookBody(form)).toEqual({
      name: "Website",
      url: "https://example.org/heron",
      events: ["track.started"],
      include_fx: true,
      enabled: false,
      content_type: "application/json",
      body_template: null,
    });
  });

  it("carries the template and its content type", () => {
    const body = webhookBody({ ...form, custom: true });
    expect(body.content_type).toBe("text/plain");
    expect(body.body_template).toBe(DEFAULT_TEMPLATE);
  });
});
