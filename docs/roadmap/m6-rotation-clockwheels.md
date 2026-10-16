# M6 – Rotation & clockwheels

**Depends on:** M4

## Scope

- Categories, separation rules, queue depth.
- Clocks as ordered slots; selection methods; enforce-rules flag; weights on play / request.
- Rule relaxation and fallback category, with logging.
- Clock scheduling by hour and day of week.
- Drag-and-drop clock editor in the UI.
- Selection log explaining why each track was picked.

## Acceptance criteria

- [ ] Simulation mode generates 7 days of programming and reports rule violations: zero, except logged relaxations.

## References

- [Rotation](../rotation.md)
- [Data model – Clockwheels](../data-model.md#clockwheels)
- Open question: exact relaxation order ([Open questions](../open-questions.md))
