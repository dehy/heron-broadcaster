# Rotation

Modeled on SAM Broadcaster's "Playlist Rotation Rules" and "Clockwheel (category rotation)". Delivered in [M6 – Rotation & clockwheels](roadmap/m6-rotation-clockwheels.md); the code lives in the `heron-rotation` crate.

## Rules

### Separation rules

Configurable, in minutes. Do not play the same:

| Rule | Default |
|---|---|
| Album within | 60 min |
| Artist within | 120 min |
| Title within | 240 min |
| Track within | 240 min |

### Queue depth

Keep N tracks queued ahead (default 20).

### Clockwheel

An ordered list of slots. Each slot has:

- a **category**;
- a **selection method**: least recently played, random or weighted;
- an **enforce rules** flag.

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
- Slots with **enforce rules** off skip the separation rules entirely.
- Every relaxed pick records its step in `play_history.relaxation_applied` and in the selection log.

**Why artist before title** (the handoff proposed title first): repeating an artist within two hours is common and accepted on radio, while hearing the same song again, even in another version, is a clear repeat to listeners.

### Scheduling

Clocks are assigned per hour and day of week (`clock_schedule` in the [data model](data-model.md)).

## Rotation model

- Rotation is a **pure function** of (library, play history, rules, current clock, current time).
- It returns the next track plus an **explanation**: which slot, which rules were applied or relaxed.
- This makes it possible to **simulate** days of programming offline and test it without audio.
