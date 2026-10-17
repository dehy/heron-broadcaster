# M6 – Rotation & clockwheels

**Depends on:** M4

## Scope

Clock construction and the way slots call their categories are the heart of a good programme: this milestone gets as much care as the audio engine.

- Categories, separation rules (title, artist, album, track), queue depth.
- Separation windows overridable per category and per clock slot.
- Clocks as ordered slots; how each slot calls its category: selection method, enforce-rules flag, separation overrides; weights on play / request.
- **Artists with several names:** artists, their names and groups; tracks linked to one or more artists; artist separation by artist, not by name. MusicBrainz relationships offered as suggestions once [M1b](m1b-library-management.md) identifies tracks.
- **Restrictions**, never relaxed: dayparting (days and hours), date ranges, and sequences (a category never directly after another), on tracks, artists and categories.
- Rule relaxation and fallback category, with logging.
- Clock scheduling by hour and day of week.
- Drag-and-drop clock editor in the UI, showing each hour as it will sound; editors for artist names and restrictions.
- Selection log explaining why each track was picked.

## Acceptance criteria

- [ ] Simulation mode generates 7 days of programming and reports rule violations: zero, except logged relaxations.
- [ ] The simulation never breaks a restriction, relaxed steps included.
- [ ] Two names of the same artist (or of an artist group counted as one) are never played closer than the artist separation, except in logged relaxations.

## References

- [Rotation](../rotation.md)
- [Data model – Clockwheels](../data-model.md#clockwheels)
- [Rotation – Rule relaxation](../rotation.md#rule-relaxation)
- [Rotation – Artists with several names](../rotation.md#artists-with-several-names), [Restrictions](../rotation.md#restrictions)
