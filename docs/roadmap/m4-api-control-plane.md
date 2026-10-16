# M4 – API & control plane

**Depends on:** M2

## Scope

- axum REST API: library, categories, queue, decks, settings.
- WebSocket: now playing, deck positions, levels, queue changes.
- Authentication (local users only for now) and roles.
- OpenAPI spec generated from code.
- Queue manager feeding the engine.

## Acceptance criteria

- [ ] Every engine action is available through the API.
- [ ] The OpenAPI spec is published and used to generate the web client types.

## References

- [Architecture – Processes](../architecture.md#processes)
- [Requirements – Outputs and integrations](../requirements.md#4-outputs-and-integrations)
- [Decisions – Authentication](../decisions.md#decisions-made-after-the-handoff)
