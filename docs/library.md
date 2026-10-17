# Library management

How Heron files, identifies and reorganizes music. Delivered in [M1b](roadmap/m1b-library-management.md) by `heron-librarian`, the only service with write access to the library.

```
incoming folder ──► heron-librarian ──────────────────────────────► music library ──► scanner, analyzer
  (drop zone)       fingerprint, AcoustID, MusicBrainz,               (read-write here,   (read-only)
                    tags, cover.jpg, journaled move                    read-only elsewhere)
```

Like iTunes' "Keep Music folder organized": drop files into the incoming folder and they end up tagged and filed under Artist / Album / Track. Nothing else in the library is touched unless you ask, and every change is journaled and can be undone.

## Folders and permissions

| Folder | Compose variable (host) | Mounted in |
|---|---|---|
| Music library | `HERON_MUSIC_DIR` | `heron-librarian` read-write; `heron-api`, `heron-analyzer`, `heron-engine` read-only |
| Incoming folder | `HERON_INCOMING_FOLDER` | `heron-librarian` only |

`heron-librarian` runs as `HERON_LIBRARIAN_USER` (`uid:gid`), which must own both folders: use the output of `id -u` and `id -g` for the owner of your music.

## The incoming folder

- **When:** at startup, after file system events (debounced 2 s), and every `HERON_INCOMING_INTERVAL` seconds (default 60). Set `HERON_INCOMING_WATCH=false` where watching does not work (some network shares).
- **What:** the same audio extensions as the scanner. Hidden files and other files (cover art, playlists, notes) are left in place.
- **Files still being copied:** files modified less than `HERON_IMPORT_SETTLE` seconds ago (default 5) wait. A file that changes while it is being imported is left for the next pass.
- **Siblings:** the files of one folder are handled together, so the files of an album agree on their release.
- Emptied folders are removed from the incoming folder.

## Identification

Identification needs the station's own **AcoustID application key** (free keys are for non-commercial use: commercial stations need a commercial key from AcoustID). Without a key, incoming files are filed using their own tags, and nothing is flagged for review.

1. **Fingerprint:** Chromaprint of the first two minutes, like `fpcalc` (`rusty-chromaprint`). The analyzer also fingerprints every track during its analysis decode (`track_fingerprints`), so existing tracks can be identified later without reading them again.
2. **AcoustID** returns the recordings that match, with a score, and the releases they are on.
3. **The release is picked from the siblings:** the release matching the most files of the folder is picked first, then the next one for the files left. A pick is applied only when it is unambiguous:
   - one release matches more files than any other; or
   - exactly one of the tied releases is titled like the files' album tag; or
   - the tied releases are editions of one release group that put every file at the same disc and track (the tags are the same whichever is picked; the earliest is).
4. **MusicBrainz** gives the release's tags, once per release (not per file).
5. **Cover Art Archive:** the release's front cover (500 px), saved as `cover.jpg` in the album folder unless one is there already.

A match is **applied automatically** when its AcoustID score is at least `min_score` (0.9) and it points to a single release. Otherwise the track is imported with its own tags and marked **needs review**, with the reason and the candidate releases (`track_identifications`):

| Reason | |
|---|---|
| `not found on AcoustID` | Unknown recording |
| `best AcoustID score … is below …` | A doubtful match |
| `matches N releases, none clearly better` | A lone file on an album and a compilation, for example |
| `the audio could not be fingerprinted` | Undecodable, or too short |
| `identification failed: …` | AcoustID or MusicBrainz unreachable three times (the folder waits 1, then 2 minutes between tries) |

They wait on the **review screen** of the web UI (*Review & organize*; `heron-librarian review` lists them too). For each track, pick one of its candidate releases, or keep the file's own tags (`kept`). A picked release is applied by the librarian like an import: MusicBrainz tags written into the file, filed where the layout says, cover saved, as a `review` batch that can be undone (the track then needs review again).

**Rate limits:** one request per second to MusicBrainz and to the Cover Art Archive, three per second to AcoustID, with a User-Agent that identifies Heron (`HeronBroadcaster/<version> ( https://heronbroadcaster.org )`). Each request waits a full interval after the previous response, so requests never arrive closer than the limit, even when one was delayed on the way. A `503` or `429` answer slows down and retries.

## Tags

Written into identified files:

| Tag | Value |
|---|---|
| Title, artist | The track on the release (artist credit as printed) |
| Album, album artist | The release |
| Track, track total, disc, disc total | The position on the release |
| Date | The release group's first release date (the original year, not the reissue's) |
| Compilation | Set when the album artist is Various Artists |
| MusicBrainz IDs | Recording, release track, release, release group, artist, album artist (Picard's tag names) |

Fields MusicBrainz leaves empty keep the file's value. Other tags (genre, comments, lyrics, …) are never touched. Cover art is embedded only when `embed_cover` is on, and only in files without one.

The previous tags are kept in `track_tag_changes`, so a change can be undone. Re-tagging keeps the track's ID, and so its analysis, categories and history: the audio did not change.

## Layout

Files are filed at `layout` under the library root. Default:

```
{album_artist}/{album}/{disc-}{track:02} {title}.{ext}
```

| Field | Value |
|---|---|
| `{album_artist}` | Album artist, else artist. `Compilations` for various-artists albums |
| `{artist}` | Artist, else album artist |
| `{album}`, `{title}`, `{year}`, `{ext}` | `{title}` defaults to the original file name |
| `{track}`, `{disc}` | Numbers; `{track:02}` pads to two digits |
| `{disc-}` | `2-` on multi-disc albums, nothing otherwise |

Missing artists and albums give `Unknown Artist` and `Unknown Album`. Names are made safe on every file system: `/ \ : * ? " < > |` and control characters become `_`, trailing dots and spaces are removed, a leading dot becomes `_` (hidden files are not imported), Windows reserved names (`CON`, `NUL`, …) get a `_`, and each component is at most 255 bytes.

**Files are never overwritten:** a name already taken (by a file, or by a track whose file is missing) gets ` (2)`, ` (3)`, … before the extension.

## Moves and the journal

Every move and tag write is recorded in `library_moves` before the file is touched, and the track is updated in the same step that marks the file in place, so the scanner never sees a track's file go missing:

| Step | Status |
|---|---|
| Record the operation (an imported file also gets its track, marked missing until the file is in place) | `pending` |
| Move the file without ever replacing another: a rename (`renameat2(RENAME_NOREPLACE)`, or a hard link and unlink where that is not supported); across file systems, or when tags change, a copy into a hidden `.heron-<id>` file next to the target, flushed and read back to verify, then renamed into place | |
| Point the track at the file, with its new stamp | `moved` (copy) or `done` (rename) |
| Remove the original of a copy | `done` |

On startup, before anything else, interrupted operations are **finished or rolled back**: a file that reached its new path (recognized by its content hash, or by being the same file) is recorded there; otherwise the operation is abandoned and the file stays where it was. The scanner skips files of an unfinished operation, and a [library lock](#coordination-with-the-scanner) keeps it from scanning in the middle of one.

The one replacement Heron makes is re-tagging a file in place: the re-tagged copy atomically replaces the original, which never leaves a half-written file.

## Reorganizing an existing library

Only on request, after a dry run: from the web UI (*Review & organize*, admins), or on the command line:

```sh
docker compose exec heron-librarian heron-librarian organize           # dry run: lists every move, changes nothing
docker compose exec heron-librarian heron-librarian organize --apply   # moves, as one batch
docker compose exec heron-librarian heron-librarian undo <batch>       # back to the previous paths
docker compose exec heron-librarian heron-librarian batches            # recent batches
```

Organizing uses the files' current tags (it does not identify them). Files that are not tracks (cover art, playlists) stay where they are; emptied folders are removed.

**Undo** moves files back to their previous paths and writes back their previous tags, byte for byte for moves. Undoing an import restores the files' own tags; the files stay in the library. A file whose previous path has been taken since is left where it is and reported. An undo interrupted by a crash is resumed by running it again. An organization interrupted by a crash leaves a partial batch: run `organize --apply` again for the rest, and undo both batches to go back.

## Settings

Stored as JSON under the `library` name in the `settings` table; missing fields take the defaults. Change them through the API (`PATCH /api/v1/settings/library` with the fields to change, admins only; the AcoustID key is never sent back, see [API](api.md)), or on the command line:

```sh
docker compose exec heron-librarian heron-librarian settings
docker compose exec heron-librarian heron-librarian settings --set '{"acoustid_key": "…"}'
```

| Setting | Default |
|---|---|
| `acoustid_key` | none: identification off |
| `layout` | `{album_artist}/{album}/{disc-}{track:02} {title}.{ext}` |
| `min_score` | 0.9 |
| `save_cover` | true |
| `embed_cover` | false |

## Work asked through the API

The web UI's review, organize and undo go through the API, which cannot write files: it queues **library tasks** (`library_tasks`) that the librarian runs one at a time, in order, next to its imports (both take the library lock for each group of file operations). Results (the dry run's moves, the batch report) are stored with the task for the web UI to show. A task running when the librarian stops is marked failed: ask again (an undo resumes where it stopped; an organization is applied once more for the rest). See [API – Library management](api.md#library-management).

## Coordination with the scanner

The scanner (in `heron-api`, read-only) and the librarian share a MariaDB named lock (`GET_LOCK`): the scanner holds it for a scan, the librarian for each group of file operations. A process that dies releases it with its connection. Since the librarian records the stamp of every file it writes, the scanner finds them unchanged and does not re-import them.

## Limitations

- The engine holds its upcoming tracks by path. The queue manager follows moves within 5 seconds ([API](api.md#queue-manager)), so only a track due to start in those seconds can be skipped.
- Existing tracks are fingerprinted but not identified yet: identification runs on incoming files only.
- Identical files dropped twice are imported twice (with a ` (2)` suffix).

Metrics: `heron_librarian_imports_total{result}` (identified, needs_review, unidentified, failed).
