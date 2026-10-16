# Workflow

How to work on Heron Broadcaster, whether you are a person or an AI agent such as Claude Code.

## Rules

- Work **one milestone at a time, in order** (see the [roadmap](roadmap/README.md)). Do not start a milestone before the previous one meets its acceptance criteria, unless the dependency column allows it.
- Treat everything in [Locked decisions](decisions.md) as fixed. If something looks wrong, stop and ask instead of working around it.
- At the end of each milestone:
  1. Demonstrate every acceptance criterion.
  2. Update the milestone status in the Notion "Roadmap" database (if the Notion connector is available).
  3. Summarize what was done, what was deferred and any open questions.
- Prefer small, reviewable commits using Conventional Commits (`feat(engine): …`, `fix(analysis): …`). See [Conventions](conventions.md).
- When a requirement is ambiguous, pick the simplest option that keeps 24/7 airing safe, note the assumption in the PR or summary, and move on.
- Questions listed in [Open questions](open-questions.md) should be raised when they become relevant instead of guessed.

## Suggested first prompt

> Read `docs/README.md`. Implement milestone M0 only (`docs/roadmap/m0-foundations.md`). When the acceptance criteria pass, summarize what you did, list any assumptions, and stop before starting M1.
