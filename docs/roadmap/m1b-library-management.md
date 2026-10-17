# M1b – Library management

**Depends on:** M1. Can run in parallel with M2. The review screen came with [M5](m5-web-ui.md).

Like iTunes' "Keep Music folder organized" and "Copy files to the Music folder when adding": you drop files into an incoming folder, and Heron identifies them, fixes their tags and files them under Artist / Album / Track.

## Scope

### Folders

- `HERON_INCOMING_DIR`: drop zone. Files are picked up once they stop changing (same settle rule as the scanner).
- `HERON_LIBRARY_DIR`: the managed library, laid out by Heron.
- Only the service that moves files and writes tags gets write access to these folders. The engine and analyzer keep read-only mounts.

### Identification

- Compute a **Chromaprint** fingerprint during the analysis decode, which already reads the whole file (for example with `rusty-chromaprint`, MIT), and store it.
- Look it up on **AcoustID**, then fetch the recording and release from **MusicBrainz**, and the cover from the **Cover Art Archive**.
  - Respect their rate limits: 1 request/s for MusicBrainz, with a User-Agent that identifies Heron.
  - Each station configures its own AcoustID application key.
- Pick the release using the track's siblings: files of the same album should agree on the release.
- **Auto-apply** when the match is confident and points to a single release. Otherwise mark the track **needs review**; it is still imported, with its own tags.

### Tags

- Write title, artist, album artist, album, track and disc numbers, year, MusicBrainz IDs and, optionally, cover art (`lofty`).
- Keep the previous tags in the database so a change can be undone.
- Writing tags changes the file stamp. The scanner must treat Heron's own writes as known changes and not re-import them as new files.

### Organizing

- Default layout: `{album_artist}/{album}/{disc-}{track:02} {title}.{ext}`, configurable.
  - Various-artist albums go to `Compilations/{album}/…`.
  - Missing tags give `Unknown Artist/Unknown Album/…`.
- Names are made safe on every file system:
  - characters `/ \ : * ? " < > |` are replaced;
  - trailing dots and spaces are removed;
  - each path component is at most 255 bytes.
- Name collisions get a ` (2)` suffix. **Files are never overwritten or deleted.**
- **Moving files:**
  - on the same file system, a rename;
  - across file systems, copy, verify, then remove the original.
  - Every move is recorded in a journal (`library_moves`), and the track's path is updated in the same step, so the scanner never sees it as missing.
- **The existing library is reorganized only on request**, after a dry run that lists every planned move. Every operation can be undone from the journal.

## Acceptance criteria

- [x] Dropping 100 files (tagged, badly tagged, untagged) into the incoming folder:
  - confidently identified files end up tagged and filed;
  - the others are imported and flagged for review.

  `crates/librarian/tests/import.rs`: 40 well-tagged and 24 badly tagged album tracks plus a 16-track compilation are identified, tagged from MusicBrainz and filed (badly tagged ones fixed by their siblings, the compilation under `Compilations/`); 10 unknown files, 6 lone files on two releases and 4 doubtful matches are imported with their own tags and flagged, each with its reason.
- [x] Interrupting the service during moves (chaos test) loses no file, overwrites none, and leaves no track pointing at a missing path.

  `crates/librarian/tests/chaos.rs`: the binary is aborted at every step of an operation and killed at random while importing (copies and renames), organizing and undoing, then restarted. After every crash, every file is still there and a file at a colliding path is untouched; at the end, every file exists exactly once, every track's path exists with its recorded stamp, and no operation or temporary file is left.
- [x] A dry run of organizing an existing library lists every move and touches nothing. Applying it and then undoing it restores the original paths and files byte for byte.

  `crates/librarian/tests/organize.rs`, also after crashes in `chaos.rs`.
- [x] Re-tagging or moving a file keeps its analysis, categories and history.

  The track keeps its ID: `organize.rs` (moves) and `import.rs` (undoing an import re-tags 80 files in place, the analysis stays).
- [x] Rate limits of AcoustID and MusicBrainz are respected (tested against a mock server).

  `import.rs`: MusicBrainz requests at least 1 s apart, one per release rather than per file; any 4 AcoustID requests span at least 1 s; every request identifies Heron in its User-Agent.

## Results

- Design: [Library management](../library.md). A new service, `heron-librarian`, holds the only read-write mount of the library; it and the scanner share a MariaDB named lock.
- Each station sets its own AcoustID key; without one, files are filed by their own tags. Covers go to `cover.jpg` (embedding optional). See the resolved [open questions](../open-questions.md).
- The analyzer now fingerprints every track (`ALGORITHM_VERSION` 2): upgrading from M1 re-analyzes the library once.
- Deferred: identifying tracks already in the library (they are fingerprinted, ready for it), the review screen ([M5](m5-web-ui.md)). Until then, review uses `heron-librarian review`. Following moves of queued tracks came with [M4](m4-api-control-plane.md).

## References

- [Import and analysis](../analysis.md)
- [Decisions – Library files](../decisions.md#decisions-made-after-the-handoff)
- [Open questions](../open-questions.md)
