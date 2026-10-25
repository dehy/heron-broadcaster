# M7 – Webhooks & output API

**Depends on:** M4

## Scope

- Webhooks on track start, end and skip, with configurable payload.
- HMAC signature, retries with backoff, delivery log.
- Public now-playing and history endpoints.

## Acceptance criteria

- [x] A test receiver gets every track change within one second.
- [x] Failed deliveries are retried.

## Delivered

- `webhooks` and `webhook_deliveries` (replacing the initial schema's placeholders), the dispatcher and the sender in `heron-api` (`crates/api/src/webhooks.rs`), the public endpoints (`crates/api/src/public.rs`), the **Webhooks** page. See [API – Webhooks](../api.md#webhooks) and [Public endpoints](../api.md#public-endpoints).
- Shown by `e2e::webhooks_report_track_changes_and_public_endpoints_answer_anyone` (every track change the engine reports, signed, within a second; skips; templates; the log, a ping and a retry; the public endpoints) and `webhooks::delivery_tests` (retries until delivered, giving up, unreachable receivers).
- Deferred: cover art on the public endpoints, and an upcoming-tracks endpoint.

## References

- [Requirements – Outputs and integrations](../requirements.md#4-outputs-and-integrations)
- [Data model – Integrations](../data-model.md#integrations-and-access)
