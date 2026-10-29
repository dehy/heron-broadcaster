# Medleys

A **medley** strings together the hooks of several songs (the most recognizable few seconds of each), with **liners** (station voice, slogans) between or over them. Heron builds medleys from a **recipe**: it picks the songs, cuts their hooks, places the liners, then mixes everything into an audio file of the library. The medley is then a track like any other: analyzed, put in a category, scheduled by clocks or fired on the FX player.

Medleys are made under *Library › Medleys* (operators and admins; viewers can see and listen).

## Settings

What every medley shares is set once, under *Settings › Medleys* (operators and admins):

| | |
|---|---|
| **Category** | The Heron category rendered medleys go in, usually a Jingle-type one, so clocks and the FX player can call it |
| **Liners** | The category of the liners; optionally a category of **opening liners** and one of **closing liners**, for the liner at the start and the one at the end of a medley (the liner category when not set) |
| **Naming** | Medleys are named after the **artists** (the default) or the **titles** of their hooks, see [Names](#names) |
| **Defaults** | The recipe of a new medley: 3 hooks, a 500 ms crossfade, hooks of 4 to 8 s, a liner every hook, between the hooks, a liner at the start and one at the end, −10 dB of ducking, the music going down during each liner from its start and back up across its end over 0.3 s each, no fade-in and no fade-out. Each medley can change them |

Medleys made before these settings existed each had their own category and liner category: the settings start with the ones they used most, the tracks already rendered keep their categories, a medley that had no liners has its liners off, and each keeps its name until it is rendered again.

## The recipe

| | |
|---|---|
| **Songs** | The categories the hooks come from, optionally only the tracks with a label |
| **Hooks** | How many (2 to 30), their order (as picked, by year, by BPM), and their length, a filter: only hooks lasting between the shortest and the longest qualify; a hook is never cut, but can be shortened by hand while editing |
| **Crossfade** | Between two parts |
| **Liners** | On or off; placed every *N* hooks, optionally also at the start and at the end; **between** the hooks (they take their own time) or **over** the music (the hooks around them play longer, the music lowered under them by the ducking level; where and how fast it goes down and comes back up is set, see [Liners over the music](#liners-over-the-music)) |
| **Fade-in** | How long the music fades in at the start of the medley (none by default), and its curve: linear, fast then slow (the music comes up quickly at first), slow then fast, or S-shaped. A liner at the start is not faded: the music comes up under it, or after it when liners go between the hooks |
| **Fade-out** | How long the music fades out at the end of the medley (none by default), and its curve: linear, fast then slow, slow then fast, or S-shaped. A liner at the end is not faded: the music fades under it, or before it when liners go between the hooks |

A song qualifies when it is enabled, its file is there, it is analyzed and audible, and it has a **hook** (imported from the previous software, see [Importing](import.md), or set on the track's page) whose length fits the recipe. Medleys never qualify, as songs or as liners (even when they go in the liner category).

## Building

1. **Build** picks the parts and shows them in order: each hook with its song and times, each liner. Nothing is written yet. A medley is built until its first render only: a rendered medley is a track the station plays, changed with **Edit** (below), never picked anew by accident.
   - The same song is never twice in a medley. Songs that are not in the medley's current version come first, so building again gives new songs while there are enough.
   - The same liner is never used twice in a row.
   - The page says how many songs qualified, when fewer than asked.
2. Each part can be **listened to** in the browser (see [Listening](#listening)), **replaced** by a song (or liner) **chosen from a list** or by another random pick, **moved** or **removed**; **Add a hook** and **Add a liner** add one chosen from the same lists. The lists hold the tracks that qualify and are not in the medley yet.

   To change a medley already rendered without picking every song anew, **Edit** its current version: the parts stay as they are until changed.

   Each part's start and end can be moved by **0.1 s** (− / +), within its file, to place a hook by ear. This changes the medley only, not the track's hook.

   The **timeline** draws the version as it will sound: the hooks with their crossfades and the liners, each with a line of its level as mixed: its fades and crossfades, and for the music the ducking under the liners, the fade-in and the fade-out. With liners over the music, it shows the extra seconds of each hook under each liner, and the point where the music passes from one hook to the next can be dragged, or moved by 0.1 s, to change how much of the liner is over each hook.

   Every part (hook or liner) can be **dragged** left or right on the timeline to move it in the medley's time; a click without moving plays from that point instead:
   - a part played after another (a hook, or a liner between the hooks) overlaps the one before it more or less, from 0 (back to back) up to the shorter of the two, instead of by the recipe's crossfade; the parts after it move with it;
   - a liner over the music moves over the music, the ducking with it; it can then cover a hook's chosen span.

   The list shows how far each part was moved, with − / + (0.1 s) and a way back to where the recipe puts it.

   **Preview** mixes the version being edited exactly as a render would, into a temporary file the library scan never sees (the hidden `medleys/.previews/` folder, next to the rendered medleys; replaced by the next preview), and plays it. The library does not change; the page says when the version changed since its preview.
3. **Render** saves the parts and asks the librarian (the only service that writes to the library, see [Library management](library.md)) to mix them into `medleys/medley-<id>.wav`, 48 kHz, 16-bit stereo:
   - each part is brought to the station's loudness target (the [playout settings](audio-engine.md#loudness)) before mixing, so the medley sounds even;
   - parts start and end with short fades, and overlap by the crossfade;
   - the result is scaled down if it would clip.
4. The library scanner finds the file: the medley becomes a track, named after the medley, in the category of the [settings](#settings). Rendering again (after editing) replaces the file in place: the track keeps its ID, categories and history, and is analyzed again. A track on air at that moment plays on from the previous file.

## Listening

The page has one player for the medley: the rendered medley, the preview of the version being edited, and each part on its own all play through it, so starting one stops the others. It has play / pause, stop, and the position over the length. While it plays, a vertical bar follows the position on the timeline (inside a part's block when a single part plays).

Clicking a point of the timeline plays from there: the rendered medley, or, while a version is edited, its preview, mixed again first when the version, or the recipe saved, changed since.

While a render runs, the page shows neither the timeline nor anything to play: the file and its layout are about to change. Each render is played from a new address, so the browser never plays a file it kept from a previous render.

Outside editing, the timeline draws the medley **as it was rendered**: the librarian saves the layout it mixed (where each part starts, the span of its file played, the length and the fade-out) with the medley, so the bar follows what is heard even after the recipe changed. A medley rendered before that layout was saved, or whose file does not last as long as its layout (by more than a quarter of a second), shows today's layout with a warning: render it again for the two to match.

## Liners over the music

A liner over the music never covers the span of a hook chosen to be heard. The hooks around it play longer instead, and the liner plays over those extra seconds only.

Under it, the music goes down by the ducking level, then comes back up. Each of the two has a length (0 to 5 s) and a place:

| | Before / after the liner | Across (a crossfade) | During the liner |
|---|---|---|---|
| **Going down** | Ends as the liner starts | Centred on the liner's start | Starts as the liner starts |
| **Coming back up** | Starts as the liner ends | Centred on the liner's end | Ends as the liner ends, as radio software brings the music back during a jingle's tail |

New medleys go down during the liner and come back up across its end, over 0.3 s each. Medleys made before these settings keep their placement, going down before the liner and coming back up during it, with a 0.3 s going down from their next render; files rendered before keep their timeline as mixed. Placed during the liner, the two never go past it nor cross: when the liner is too short for both, they share it. Liners in a row keep the music down from the first to the last.

- **Between two hooks**, the hook before plays on past its end and the hook after starts before its start. The gap they make lasts the liner, plus the crossfade between the two hooks, plus what the going down takes before the liner and the coming back up after it, so both chosen spans are heard whole and never ducked: the span of the hook before ends as the music starts going down, the span of the hook after starts once it is back up.
- By default the liner is over as much of the hook before as of the hook after (50/50): a 2 s liner after a hook from 2:17.4 to 2:25.4 plays that hook on to about 2:26.4, and starts the next one about 1 s before its start. The share is set for each liner on the timeline, from all of it over the hook before to all of it over the hook after; the two extensions follow.
- **At the start** of the medley, the first hook starts earlier by the liner's whole length (and what the coming back up takes after it), its span starting once the music is back up; **at the end**, the last hook plays on under the liner for its whole length.
- Several liners in a row between the same hooks play one after the other in the same gap, with the share of the first.
- An extension stops at the start or the end of its file; the other hook takes what is missing.

The parts keep the spans chosen to be heard; the extensions are worked out at each preview and render.

## Names

A medley is named when it is rendered: `Medley`, the year and month of the render (in the station's time zone), and the artists (or titles, as [set](#settings)) of its hooks in order, in their own case. For three hooks by Helena, Panic! At The Disco and Mabel:

- by artist: `Medley 2026-10 (Helena - Panic! At The Disco - Mabel)`;
- by title: `Medley 2026-10 (Mauvais garçon - High Hopes - Don't Call Me Up)`.

An artist (or title) that comes twice is named once; a hook without one is left out of the name. A name longer than a track title allows (255 characters) ends with `…` after the last name that fits. When another medley has the name already, ` (2)`, ` (3)`… is added.

Until its first render a medley has no name: the page shows the name it would get. Rendering again after an edit names it again, and its track's title follows.

## On air

A medley track **sends no metadata**: while it plays, the stream servers, the public now-playing and the public history keep the last song (as jingles do, see [Outputs – Metadata](outputs.md#metadata)). It is played like any other track otherwise.

## Duplicating

**Duplicate** makes a new medley with the same recipe and parts. It is not rendered: no name, no file and no track until it is, so it can be edited, or built anew, then rendered. That is how to make a variant of a medley, or a new one from the same recipe.

## Deleting

Deleting a medley removes its recipe; its rendered track stays in the library (delete it from the track list if it is no longer wanted). A song leaving the library does not change a rendered medley: it is used again only when the medley is built again.

## API

| | |
|---|---|
| `GET /medleys`, `POST /medleys` | The medleys, with their name (`null` until rendered), recipe, parts, track and last render; a new medley (`{"recipe"}`) |
| `GET`, `PUT`, `DELETE /medleys/{id}` | One medley; its recipe |
| `GET`, `PUT /settings/medleys` | The [settings](#settings): `category_id`, `liner_category_id`, `opening_liner_category_id`, `closing_liner_category_id`, `name_from` (`artists` or `titles`), `defaults` (a recipe without its songs) |
| `POST /medleys/{id}/plan` | Picks the parts (writes nothing): `{"parts": [{"kind": "hook" or "liner", "track_id", "from_ms", "to_ms", …}], "qualified"}`; `409` once the medley is rendered |
| `POST /medleys/{id}/duplicate` | A new medley with the same recipe and parts, not rendered |
| `POST /medleys/{id}/pick` | One more part of a kind, other than the tracks given (`{"kind", "exclude": [track IDs], "slot"}`), to replace one; for a liner, `slot` is `start`, `end` or `between` (the default), where the liner goes |
| `GET /medleys/{id}/candidates?kind=hook` | Every track that can be a part of that kind (`hook` or `liner`, with `slot` for a liner), with the span it would take, by artist and title |
| `POST /medleys/{id}/preview` | `{"parts": […]}` (each part `{"kind", "track_id", "from_ms", "to_ms", "shift_ms", "move_ms"}`; `shift_ms` for a liner over the music: how far from the middle of its gap the music passes from the hook before to the hook after, positive for more of the liner over the hook before; `move_ms`: how far the part was moved in the medley from where the recipe puts it, negative for earlier): mixes them into the medley's preview (a library task, `preview_medley`); nothing is saved |
| `GET /medleys/{id}/preview` | The last preview's audio (WAV, with range requests) |
| `POST /medleys/{id}/render` | `{"parts": […]}`: saves them, names the medley and queues the rendering (a library task, `render_medley`) |

Operators and admins change medleys; every signed-in user reads them.
