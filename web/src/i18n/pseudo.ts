// SPDX-License-Identifier: AGPL-3.0-or-later

// The pseudo-locale (`?lang=pseudo`, in development): English with accented
// letters, a third longer and in brackets, so text that escaped the catalogs
// stands out and layouts are tried with longer words. Parameters stay as
// they are.

import en from "./en";
import type { Catalog, Message } from ".";

const ACCENTS: Record<string, string> = {
  a: "à", b: "ƀ", c: "ç", d: "ð", e: "é", f: "ƒ", g: "ĝ", h: "ĥ", i: "î", j: "ĵ", k: "ķ", l: "ļ", m: "ɱ",
  n: "ñ", o: "ô", p: "þ", q: "ǫ", r: "ŕ", s: "š", t: "ţ", u: "û", v: "ṽ", w: "ŵ", x: "ẋ", y: "ý", z: "ž",
  A: "À", B: "Ɓ", C: "Ç", D: "Ð", E: "É", F: "Ƒ", G: "Ĝ", H: "Ĥ", I: "Î", J: "Ĵ", K: "Ķ", L: "Ļ", M: "Ṁ",
  N: "Ñ", O: "Ô", P: "Þ", Q: "Ǫ", R: "Ŕ", S: "Š", T: "Ţ", U: "Û", V: "Ṽ", W: "Ŵ", X: "Ẋ", Y: "Ý", Z: "Ž",
};

/** `Save` → `[Šàṽé ··]`; `{count} tracks` keeps `{count}`. */
export function pseudoText(text: string): string {
  const accented = text
    .split(/(\{\w+\})/g)
    .map((part, index) => (index % 2 === 1 ? part : [...part].map((c) => ACCENTS[c] ?? c).join("")))
    .join("");
  const padding = "·".repeat(Math.max(1, Math.ceil(text.length / 3)));
  return `[${accented} ${padding}]`;
}

function pseudoMessage(message: Message): Message {
  if (typeof message === "string") {
    return pseudoText(message);
  }
  return Object.fromEntries(
    Object.entries(message).map(([form, text]) => [form, pseudoText(text)]),
  ) as Message;
}

const pseudo = Object.fromEntries(
  Object.entries(en).map(([key, message]) => [key, pseudoMessage(message)]),
) as Catalog;

export default pseudo;
