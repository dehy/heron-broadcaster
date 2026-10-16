# Conventions

## Rust

- `rustfmt`, and `clippy` with warnings denied in CI.
- No `unwrap()` / `expect()` outside tests and startup code.
- Errors: `thiserror` in libraries, `anyhow` (or equivalent) only at binary boundaries.
- Structured logging with `tracing`.
- A `/metrics` endpoint on each binary.

## Licensing

- SPDX header in every source file:

  ```rust
  // SPDX-License-Identifier: AGPL-3.0-or-later
  ```

## Web

- TypeScript strict mode.
- API client types generated from the OpenAPI spec.

## Tests

- Unit tests per crate.
- Generated audio fixtures for analysis and DSP: sine waves, silence, blips, quiet outros.
- Integration tests against MariaDB in Docker.
- Chaos tests for reliability.

## Commits

- [Conventional Commits](https://www.conventionalcommits.org/), scoped by crate or area: `feat(engine): …`, `fix(analysis): …`.
- Small, reviewable commits.
