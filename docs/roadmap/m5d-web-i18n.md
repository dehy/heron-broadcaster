# M5d – Web UI translations

**Depends on:** M5. Pages added by later milestones are translated as they arrive.

The web UI in more than one language, starting with **English and French**. Today every label is written in English in the components, and dates and numbers follow the browser's locale while relative times are forced to English. With M5d, each user picks a language, every text the interface writes is translated, and dates, numbers and plurals follow the chosen language.

## Scope

### Languages

- English is the source language; French is the first translation.
- Each user chooses a language on the **Account** page: *System* (the default) follows the browser (`navigator.languages`, first supported match, English otherwise), or a fixed language.
- Like the theme ([Web UI – Themes](../web.md#themes)), the choice is saved per user by the API (`PUT /api/v1/auth/preferences`, a `language` next to `theme`, stored on `users`) and remembered by the browser, so the first paint and the sign-in page already use it.
- `<html lang>` follows the language, for screen readers and the browser's spell checker and hyphenation.

### Messages

- Messages live in one catalog per language under `web/src/i18n/`, keyed by id. The English catalog defines the keys and their parameters; the others are typed against it, so `npm run typecheck` fails on a missing or misspelled key.
- No i18n framework: a small module (`t()` and a hook) on the browser's `Intl` API. With two languages, typed catalogs and `Intl` doing the formatting, a library would add a dependency and a build step for little. Plurals use `Intl.PluralRules`, so French says *0 piste, 1 piste, 2 pistes* and English *0 tracks*.
- Parameters are named (`{count} tracks, {analyzed} analyzed`), never concatenated in the components, so a translation can reorder them.
- Catalogs other than English are loaded only when chosen, so English users download nothing more.
- A **pseudo-locale** in development (`?lang=pseudo`: accented, lengthened text such as `[Ŏñ àîŕ ·····]`) shows any text that escaped the catalogs and any layout that breaks with longer words.

### Formatting

- Dates, times, numbers, file sizes, levels and relative times (`format.ts`, `rotation.ts`, the history days) use the chosen language instead of the browser's locale or a fixed `"en"`: `-1,5 dB` and *il y a 5 minutes* in French.
- Duration fields accept the language's decimal separator (`1:23,5` in French) as well as the dot.
- Day and month names (schedule, history, restrictions) come from `Intl`, not from the catalogs.
- Only the language changes, never the time zone, so no hour or day boundary moves. Each display keeps the zone it uses today:
  - Timestamps (`clockTime`, `dateTime`) and the history's days: the browser's time zone.
  - The schedule and the rotation: the station's time zone. Dates computed by `rotation.ts` are formatted in UTC.

### What is not translated

- What the station's people wrote or the files carry: track tags, artist, category, clock and output names.
- Messages the API sends (`message` in its errors, the engine's reasons): the web translates the error codes it knows (`unauthorized`, `forbidden`, `too_many_requests`, `engine_unavailable`, `internal`) and shows the others' English message as it is. Values the API sends as codes (engine states, how a play ended, the rotation rules held or relaxed, output states, roles) are translated by the web.
- Logs, the API's OpenAPI document, the command-line tools and the documentation stay in English.

### Adding a language

- One catalog file and one line in the list of languages. `docs/web.md` explains how, and how to check it with the catalog test and the pseudo-locale.

## Acceptance criteria

- [ ] With the pseudo-locale, every page and dialog (signed in as admin, so every control shows) has no untranslated text, and the layout holds at phone width.
- [ ] A test checks that every catalog has exactly the English keys, with the same parameters.
- [ ] Choosing French on the Account page switches the interface at once, survives a reload with the first paint already in French, and follows the user to another browser; *System* follows the browser's languages.
- [ ] In French, dates, times, numbers, levels, relative times and plurals are written the French way (unit tests in `format.test.ts`), and duration fields accept `1:23,5`.
- [ ] An English-only session downloads no other catalog.

## References

- [Web UI](../web.md)
- [API – Authentication](../api.md#authentication)
- [Data model – Integrations and access](../data-model.md#integrations-and-access)
- [M5 – Web UI](m5-web-ui.md)
