# M0 – Foundations

**Depends on:** —

## Scope

- Cargo workspace with the crates from the [repository layout](../architecture.md#repository-layout) (empty but compiling).
- React + Vite + TypeScript app in `web/`.
- Docker Compose with `heron-engine`, `heron-api`, `heron-analyzer`, `heron-web`, `mariadb`; music library and fallback directory mounted as volumes.
- `sqlx` migrations for the initial schema (see [Data model](../data-model.md)).
- CI: fmt, clippy, tests, Docker builds.
- AGPL-3.0 `LICENSE`, SPDX headers, README with quick start.

## Acceptance criteria

- [x] `docker compose up` starts all services; migrations run automatically.
- [x] CI is green.

## References

- [Architecture](../architecture.md)
- [Data model](../data-model.md)
- [Conventions](../conventions.md)
- [Locked decisions](../decisions.md)
