# Medleys

A **medley** strings together the hooks of several songs (the most recognizable few seconds of each), with **liners** (station voice, slogans) between or over them. Heron builds medleys from a **recipe**: it picks the songs, cuts their hooks, places the liners, then mixes everything into an audio file of the library. The medley is then a track like any other: analyzed, put in a category, scheduled by clocks or fired on the FX player.

Medleys are made under *Library › Medleys* (operators and admins; viewers can see and listen).

## The recipe

| | |
|---|---|
| **Name** | Shown in the library (the medley track's title) |
| **Category** | The Heron category the rendered medley goes in, usually a Jingle-type one, so clocks and the FX player can call it |
| **Songs** | The categories the hooks come from, optionally only the tracks with a label |
| **Hooks** | How many (2 to 30), their order (as picked, by year, by BPM), and their length, a filter: only hooks lasting between the shortest and the longest qualify (4 to 8 s by default); a hook is never cut, but can be shortened by hand while editing |
| **Crossfade** | Between two parts, 500 ms by default |
| **Liners** | A category (none for no liners), placed every *N* hooks, optionally also at the start and at the end; **between** the hooks (they take their own time) or **over** the music (each one starts on the transition, the music lowered under it by the ducking level, −10 dB by default) |

A song qualifies when it is enabled, its file is there, it is analyzed and audible, and it has a **hook** (imported from the previous software, see [Importing](import.md), or set on the track's page) whose length fits the recipe. The medley's own tracks never qualify.

## Building

1. **Build** picks the parts and shows them in order: each hook with its song and times, each liner. Nothing is written yet. A medley is built until its first render only: a rendered medley is a track the station plays, changed with **Edit** (below), never picked anew by accident.
   - The same song is never twice in a medley. Songs that are not in the medley's current version come first, so building again gives new songs while there are enough.
   - The same liner is never used twice in a row.
   - The page says how many songs qualified, when fewer than asked.
2. Each part can be **listened to** in the browser, **replaced** by a song (or liner) **chosen from a list** or by another random pick, **moved** or **removed**; **Add a hook** and **Add a liner** add one chosen from the same lists. The lists hold the tracks that qualify and are not in the medley yet.

   To change a medley already rendered without picking every song anew, **Edit** its current version: the parts stay as they are until changed.

   Each part's start and end can be moved by **0.1 s** (− / +), within its file, to place a hook by ear. This changes the medley only, not the track's hook.

   **Preview** mixes the version being edited exactly as a render would, into a temporary file the library scan never sees (the hidden `medleys/.previews/` folder, next to the rendered medleys; replaced by the next preview), and plays it. The library does not change; the page says when the version changed since its preview.
3. **Render** saves the parts and asks the librarian (the only service that writes to the library, see [Library management](library.md)) to mix them into `medleys/medley-<id>.wav`, 48 kHz, 16-bit stereo:
   - each part is brought to the station's loudness target (the [playout settings](audio-engine.md#loudness)) before mixing, so the medley sounds even;
   - parts start and end with short fades, and overlap by the crossfade;
   - the result is scaled down if it would clip.
4. The library scanner finds the file: the medley becomes a track, named after the medley, in the recipe's category. Rendering again (after editing) replaces the file in place: the track keeps its ID, categories and history, and is analyzed again. A track on air at that moment plays on from the previous file.

## On air

A medley track **sends no metadata**: while it plays, the stream servers, the public now-playing and the public history keep the last song (as jingles do, see [Outputs – Metadata](outputs.md#metadata)). It is played like any other track otherwise.

## Duplicating

**Duplicate** makes a new medley named after the first with "(copy)" ("(copy 2)"… when taken), with the same recipe, category and parts. It is not rendered: no file and no track until it is, so it can be edited, or built anew, then rendered. That is how to make a variant of a medley, or a new one from the same recipe.

## Deleting

Deleting a medley removes its recipe; its rendered track stays in the library (delete it from the track list if it is no longer wanted). A song leaving the library does not change a rendered medley: it is used again only when the medley is built again.

## API

| | |
|---|---|
| `GET /medleys`, `POST /medleys` | The medleys, with their recipe, parts, track and last render; a new medley |
| `GET`, `PUT`, `DELETE /medleys/{id}` | One medley; its recipe |
| `POST /medleys/{id}/plan` | Picks the parts (writes nothing): `{"parts": [{"kind": "hook" or "liner", "track_id", "from_ms", "to_ms", …}], "qualified"}`; `409` once the medley is rendered |
| `POST /medleys/{id}/duplicate` | `{"name"}`: a new medley with the same recipe, category and parts, not rendered |
| `POST /medleys/{id}/pick` | One more part of a kind, other than the tracks given (`{"kind", "exclude": [track IDs]}`), to replace one |
| `GET /medleys/{id}/candidates?kind=hook` | Every track that can be a part of that kind (`hook` or `liner`), with the span it would take, by artist and title |
| `POST /medleys/{id}/preview` | `{"parts": […]}`: mixes them into the medley's preview (a library task, `preview_medley`); nothing is saved |
| `GET /medleys/{id}/preview` | The last preview's audio (WAV, with range requests) |
| `POST /medleys/{id}/render` | `{"parts": […]}`: saves them and queues the rendering (a library task, `render_medley`) |

Operators and admins change medleys; every signed-in user reads them.
