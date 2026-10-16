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

When no track satisfies the rules, relax them in a defined order, then fall back to a safe category. Log every relaxation. **The queue must never be empty.**

Initial proposal for the order, still to confirm (see [Open questions](open-questions.md)):

1. Album separation
2. Title separation
3. Artist separation
4. Track separation
5. Fallback category

### Scheduling

Clocks are assigned per hour and day of week (`clock_schedule` in the [data model](data-model.md)).

## Rotation model

- Rotation is a **pure function** of (library, play history, rules, current clock, current time).
- It returns the next track plus an **explanation**: which slot, which rules were applied or relaxed.
- This makes it possible to **simulate** days of programming offline and test it without audio.
