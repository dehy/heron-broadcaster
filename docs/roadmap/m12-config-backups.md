# M12 – Configuration backups

**Depends on:** M4. EQ presets are saved once [M9](m9-deck-eq.md) is in.

## Why

A station's configuration is set by hand, over months: clocks, the schedule, categories, restrictions, outputs and their passwords, settings, webhooks, users. Losing it to a bad change, a broken disk or a database restored from the wrong day costs far more than the music library, which rebuilds itself from the files. Design: [Configuration backups](../backups.md).

## Scope

- **Daily backup** by `heron-api` of the station's configuration to a JSON file, at a set time (03:30 station time by default); the last 7 days kept (a setting, 1 to 90). Secrets included, the file readable by the API's user only.
- **Manual backup** at any time, kept until deleted.
- **Restore** of a whole backup, by admins, from the web UI, the API or `heron-api backup restore`: checked first, the current configuration backed up first, applied in one transaction, sessions closed, the engine updated at once. Tracks and artists referred to by path and name, so a backup restores on top of a rebuilt library.
- **List, download, upload, delete** backups.
- **Backups page** in the web UI (admins).

## Out of scope

- What operators set on tracks (categories, weights, labels, overrides, mood, BPM), the play history and listener statistics: database backups cover them.
- Restoring part of a backup (only the clocks, only the outputs).
- Sending backups elsewhere (S3, another machine): copy the `backups` volume with the tools of your choice.
- Encrypting the files.

## Deliverables

- `heron-api`: the backup file (format 1), the daily schedule and retention, `backup create|list|restore` subcommands, routes under `/api/v1/backups`, the `backups` settings group.
- `docker-compose.yml`: the `backups` volume on `heron-api`, `HERON_BACKUP_DIR`.
- `heron-web`: the Backups page.
- Docs: [Configuration backups](../backups.md), [API](../api.md), [Data model](../data-model.md#settings), [Web UI](../web.md#pages), the README.

## Acceptance criteria

- [ ] A station configured with every kind of setting saved in the backup is backed up, its database dropped, Heron started on an empty database and the library scanned; restoring the backup gives back every setting, output (streaming without retyping a password), clock, schedule, category, restriction, label, webhook, user and API token, identical through the API.
- [ ] Restrictions on tracks and artists are restored by path and name onto the rebuilt library; one whose track is gone is reported, not restored.
- [ ] The daily backup runs at its time in the station's time zone, catches up when the API was down at that time, and keeps exactly the last 7 daily backups; manual and pre-restore backups are kept as described.
- [ ] A restore is undone by restoring the backup it took first.
- [ ] A corrupt, truncated or newer-format file is refused with a clear message and changes nothing; a crash during a restore leaves the configuration as it was before it.
- [ ] Backup files are written atomically and readable by the API's user only; only admins can list, download, upload, restore or delete them.
- [ ] The engine plays on through a restore; outputs whose settings did not change do not reconnect.

## References

- [Configuration backups](../backups.md)
- [API – Engine configuration](../api.md#engine-configuration)
- [Data model](../data-model.md)
