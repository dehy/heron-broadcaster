# Rotation

Modeled on SAM Broadcaster's "Playlist Rotation Rules" and "Clockwheel (category rotation)". Delivered in [M6 – Rotation & clockwheels](roadmap/m6-rotation-clockwheels.md); the code lives in the `heron-rotation` crate.

What makes a programme sound good is mostly invisible to listeners: how the clocks are built, how each slot calls its category, the restrictions, and the time between two plays of a title or an artist. This is the heart of the product, and it deserves as much care as the audio.

## Rules

### Separation rules

Configurable, in minutes. Do not play the same:

| Rule | Default |
|---|---|
| Album within | 60 min |
| Artist within | 120 min |
| Title within | 240 min |
| Track within | 240 min |

Separation windows can be overridden per category (a power rotation category repeats its titles sooner than a gold category) and per clock slot.

Artist separation compares **artists, not names**: see [Artists with several names](#artists-with-several-names).

### Artists with several names

The same artist often appears under several names: solo name, band, side projects, spellings (`Neal Morse`, `The Neal Morse Band`). Comparing tag strings would play them back to back. So:

- An **artist** has one or more **names**. Tracks are linked to artists through their tags: a name links the track to its artist.
- A track can have several artists (featuring, duets, collaborations); artist separation applies to each of them.
- Artists can be **grouped** (a band and its members' solo projects) so separation treats the group as one artist. Whether a group counts as one artist is a setting of the group.
- Names are matched case-insensitively and ignoring accents and extra spaces; anything less obvious is linked by hand in the UI, never guessed silently.
- When tracks are identified ([M1b](roadmap/m1b-library-management.md)), MusicBrainz artist IDs and relationships ("member of", "performs as") are offered as suggestions to review.

### Restrictions

Restrictions are hard rules: unlike separation, **they are never relaxed**. They apply to a track, an artist or a category:

- **dayparting:** allowed or banned on given days and hours (no explicit lyrics before 22:00, no long tracks during the morning show);
- **date ranges:** seasonal content only in its season, or a track banned for a while;
- **sequence:** a category never directly after another (no two jingles in a row, no ballad after a ballad).

When a slot finds no candidate that respects the restrictions, it goes through the relaxation steps of the separation rules, and then to the fallback category, still respecting the restrictions. The engine's fallback directory is the last resort.

### Queue depth

Keep N tracks queued ahead (default 20).

### Clockwheel

An ordered list of slots. Each slot **calls a category**, and how it calls it matters as much as which category it is. Each slot has:

- a **category**;
- a **selection method**: least recently played, random or weighted;
- an **enforce rules** flag;
- separation overrides for this slot (see [Separation rules](#separation-rules)).

Building good clocks has to be easy: the editor shows each hour as it will sound (slots, expected durations, categories running short), and a clock can be simulated before it goes on air.

Clockwheels are data stored in the database and edited in the UI. There is no scripting language (see [Locked decisions](decisions.md)).

### Weights

Configurable weight reduction on play and increase on request.

### Rule relaxation

When no track satisfies the rules, relax them step by step, then fall back to a safe category. Log every relaxation. **The queue must never be empty.**

The rule listeners notice least is relaxed first, and each rule is **halved before it is dropped**, so a small library gets "a bit less separation" rather than losing a rule outright.

| Step | Relaxation | With the default windows |
|---|---|---|
| 0 | All rules as configured | album 60, artist 120, title 240, track 240 min |
| 1 | Album separation halved | album 30 |
| 2 | Album separation dropped | album off |
| 3 | Artist separation halved | artist 60 |
| 4 | Artist separation dropped | artist off |
| 5 | Title separation halved | title 120 |
| 6 | Title separation dropped | title off |
| 7 | Track separation halved | track 120 |
| 8 | Track separation quartered | track 60 |
| 9 | Fallback category, track separation still at a quarter | |
| 10 | Fallback category, least recently played track, no rules | |

- Rotation tries the steps in order, stops at the first one that yields at least one candidate, then applies the slot's selection method to those candidates.
- Track separation is never dropped inside the slot's own category: playing the exact same recording again is the most audible repeat.
- Step 10 cannot fail while the fallback category has a track; picking the least recently played one still avoids an immediate repeat. If the fallback category is empty, the engine's fallback directory takes over.
- Slots with **enforce rules** off skip the separation rules entirely. Restrictions still apply.
- Every relaxed pick records its step in `play_history.relaxation_applied` and in the selection log.

**Why artist before title** (the handoff proposed title first): repeating an artist within two hours is common and accepted on radio, while hearing the same song again, even in another version, is a clear repeat to listeners.

### Scheduling

Clocks are assigned per hour and day of week (`clock_schedule` in the [data model](data-model.md)).

## Rotation model

- Rotation is a **pure function** of (library, play history, rules, current clock, current time).
- It returns the next track plus an **explanation**: which slot, which rules and restrictions were applied, which were relaxed.
- This makes it possible to **simulate** days of programming offline and test it without audio.
