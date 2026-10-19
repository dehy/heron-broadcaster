// SPDX-License-Identifier: AGPL-3.0-or-later

import { describe, expect, it } from "vitest";
import en from "./en";
import { LANGUAGES, type Message } from ".";
import pseudo from "./pseudo";

/** The `{parameters}` of a message, plural forms together, sorted. */
function params(message: Message): string[] {
  const texts = typeof message === "string" ? [message] : Object.values(message);
  const names = new Set<string>(typeof message === "string" ? [] : ["count"]);
  for (const text of texts) {
    for (const [, name] of text.matchAll(/\{(\w+)\}/g)) {
      names.add(name ?? "");
    }
  }
  return [...names].sort();
}

function check(name: string, catalog: Record<string, Message>) {
  expect(Object.keys(catalog).sort(), `${name}: keys`).toEqual(Object.keys(en).sort());
  for (const [key, message] of Object.entries(en)) {
    const translated = catalog[key];
    if (translated === undefined) {
      continue;
    }
    expect(params(translated), `${name}: parameters of ${key}`).toEqual(params(message));
    expect(typeof translated, `${name}: ${key} plural or not`).toBe(typeof message);
    if (typeof translated !== "string") {
      expect(translated.other, `${name}: ${key} needs an "other" form`).toBeTruthy();
    }
  }
}

describe("catalogs", () => {
  for (const language of LANGUAGES) {
    it(`${language.name} has exactly the English keys, with the same parameters`, async () => {
      check(language.code, await language.load());
    });
  }

  it("the pseudo-locale keeps the parameters", () => {
    check("pseudo", pseudo);
    expect(pseudo["common.save"]).toMatch(/^\[Š.+·+\]$/);
  });
});
