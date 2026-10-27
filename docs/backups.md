# Configuration backups

What makes a station hard to rebuild is what people set by hand: its clocks and schedule, categories and restrictions, outputs and their passwords, settings, webhooks, users. The music library rebuilds itself from the files (scan and analysis); that configuration does not. `heron-api` saves it every day to a file, keeps the last days, and restores any of them. Built in [M12](roadmap/m12-config-backups.md).

These files complement database backups, they do not replace them: they are small, readable and can be restored on their own, but they leave out the library's data and the history.

## What a backup holds

| Saved | Left out |
|---|---|
| Every settings group (playout, deck EQ, analysis, library, queue, rotation, station, programming, statistics) | Tracks, their analysis, waveforms, fingerprints, identification |
| Encoder profiles and outputs, with their passwords and statistics credentials | What operators set on tracks: categories, weights, labels, overrides, mood, BPM |
| EQ presets | The play history, the selection log, listener statistics |
| Categories, clocks and their slots, the weekly schedule | Library tasks, batches and the trash journal |
| Restrictions | The buffer zone |
| Labels (without their tracks) | Webhook deliveries |
| Artists curated by hand: names moved, merged, marked similar | Sessions (everyone signs in again after a restore) |
| Webhooks, with their secrets | |
| Users (password hashes, roles, preferences) and API tokens (hashes) | |

**Secrets are in the file.** Output and stream server passwords, webhook secrets, the AcoustID key, password and token hashes: a restore gives back a working station, with nothing to type again. The files are written readable by the API's user only (`0600`). Treat them like the database: do not mail them, and keep copies off the machine somewhere private.

**Tracks and artists are referred to by what does not change.** A restriction on a track names its library path; one on an artist, the artist's name. On a restore they are matched with the tracks and artists of the database; a restriction whose track or artist is not found is left out, and listed in the restore's report.

## The file

One JSON document per backup, named after its time: `heron-config-2026-11-03T03-30-00Z.json`.

```json
{
  "format": 1,
  "heron_version": "0.12.0",
  "schema": 20261201000000,
  "created_at": "2026-11-03T03:30:00Z",
  "kind": "daily",
  "station": "My station",
  "settings": {"playout": {…}, "eq": {…}, …},
  "encoder_profiles": […],
  "outputs": […],
  …
}
```

- `format` is the version of this document's layout; `schema`, the last database migration of the Heron that wrote it.
- `kind`: `daily`, `manual` (backed up on request), `before_restore` (taken before a restore), `uploaded`.
- A Heron restores files of its own format and older ones. A file written by a newer Heron is refused, with its version in the message.

## Where and when

- **Folder:** `HERON_BACKUP_DIR` (`/var/lib/heron/backups` in the image), the `backups` volume of `docker-compose.yml`. Only `heron-api` mounts it.
- **Daily backup:** at 03:30 in the station's time zone (`backups.time`). A day the API is down is caught up when it starts, if the last daily backup is more than a day old.
- **Kept:** the last 7 daily backups (`backups.keep_days`, 1 to 90), the backups taken before the last 7 restores, and every manual backup until it is deleted. Older daily backups are deleted after a successful one, never before.
- **Manual backup:** *Back up now* on the Backups page, `POST /backups`, or `heron-api backup create`.
- A backup is written to a temporary file, synced, then renamed: a crash never leaves half a file under a backup's name.

## Restoring

Admins only. From the **Backups** page (pick a backup, or upload a file), from the API (`POST /backups/{name}/restore`, `POST /backups/restore` with a file), or on the command line:

```sh
docker compose exec heron-api heron-api backup list
docker compose exec heron-api heron-api backup restore heron-config-2026-11-03T03-30-00Z.json
```

1. The file is checked first: its format, its schema, every part of it. A file that does not pass changes nothing.
2. The current configuration is backed up (`before_restore`), so a restore can itself be undone.
3. Everything the file holds replaces what the database has, in one transaction: a restore applies whole or not at all. What the file does not hold (tracks, history…) is not touched.
4. Every session is closed: sign in again, with the users of the backup. When no admin can sign in afterwards, `heron-api user passwd` sets a password from the command line.
5. The engine gets the restored playout settings, deck EQ and outputs at once, as after any change ([API – Engine configuration](api.md#engine-configuration)); outputs whose settings did not change keep streaming.

The restore's report lists what it replaced, counted by kind, and anything it left out (restrictions whose track or artist is gone).

**After losing the database:** start Heron with an empty database, let it scan and analyze the library, then restore the last backup: the configuration comes back on top of the rebuilt library. What operators set on tracks (categories, labels, overrides) is not in these files: restore a database backup for that.

## Downloading and uploading

- `GET /backups` lists the backups (name, kind, time, size); `GET /backups/{name}` downloads one; `DELETE /backups/{name}` deletes it. Admins only: the files hold secrets.
- Uploading a file restores it; it is kept as an `uploaded` backup.

## Settings

`backups` in the `settings` table:

| Setting | Default |
|---|---|
| `enabled` | true |
| `time` | `03:30`, in the station's time zone |
| `keep_days` | 7 |
