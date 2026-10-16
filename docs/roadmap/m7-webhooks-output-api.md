# M7 – Webhooks & output API

**Depends on:** M4

## Scope

- Webhooks on track start, end and skip, with configurable payload.
- HMAC signature, retries with backoff, delivery log.
- Public now-playing and history endpoints.

## Acceptance criteria

- [ ] A test receiver gets every track change within one second.
- [ ] Failed deliveries are retried.

## References

- [Requirements – Outputs and integrations](../requirements.md#4-outputs-and-integrations)
- [Data model – Integrations](../data-model.md#integrations-and-access)
