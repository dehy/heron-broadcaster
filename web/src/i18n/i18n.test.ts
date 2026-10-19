// SPDX-License-Identifier: AGPL-3.0-or-later

import { afterEach, describe, expect, it } from "vitest";
import { number, rich, setLanguage, systemLanguage, t, toSetting } from ".";

afterEach(async () => {
  await setLanguage("en");
});

describe("messages", () => {
  it("picks plural forms by the language's rules", async () => {
    expect(t("common.tracks", { count: 0 })).toBe("0 tracks");
    expect(t("common.tracks", { count: 1 })).toBe("1 track");
    await setLanguage("fr");
    expect(t("common.tracks", { count: 0 })).toBe("0 piste");
    expect(t("common.tracks", { count: 1 })).toBe("1 piste");
    expect(t("common.tracks", { count: 2 })).toBe("2 pistes");
  });

  it("writes numbers the language's way", async () => {
    expect(t("common.tracks", { count: 1234 })).toBe(`${number(1234)} tracks`);
    await setLanguage("fr");
    expect(t("common.tracks", { count: 1234 })).toBe(`${number(1234)} pistes`);
    expect(number(1234.5)).toMatch(/^1\s234,5$/);
  });

  it("fills in named parameters, in the translation's order", async () => {
    await setLanguage("fr");
    expect(t("library.page", { page: 2, pages: 7 })).toBe("Page 2 sur 7");
    expect(t("artists.pair", { artist: "A", other: "B" })).toBe("A et B");
  });

  it("puts elements in rich messages", () => {
    const node = rich("app.noPage", { back: "LINK" });
    expect(node).toBeTruthy();
  });
});

describe("choosing the language", () => {
  it("follows the browser's languages, English otherwise", () => {
    expect(systemLanguage(["fr-CA", "en-US"])).toBe("fr");
    expect(systemLanguage(["de-DE", "fr"])).toBe("fr");
    expect(systemLanguage(["de-DE", "it"])).toBe("en");
    expect(systemLanguage([])).toBe("en");
  });

  it("reads settings saved elsewhere", () => {
    expect(toSetting("fr")).toBe("fr");
    expect(toSetting("system")).toBe("system");
    expect(toSetting("pt-BR")).toBe("system");
    expect(toSetting(null)).toBe("system");
  });
});
