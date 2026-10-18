# M5b – Track details, labels and tags

**Depends on:** M5.

What a music programmer looks at every day: when a track was added, when it last played, how often, at what bitrate; its label, ISRC, era, BPM, mood and sound code; keywords to sort the library; and tags fixed from the browser.

## Scope

### Track details

- Each track shows, in the library and on its page:
  - **date added** to the station (when the scanner first found the file);
  - **last played** and **number of plays**, from the play history (the FX player aside). What counts as a play is a setting: by default a play that lasted at least 30 s, so skipped tracks do not count;
  - the **file**: bitrate, CBR or VBR, sample rate, channels, codec, size, read by the scanner with the tags.
- The library can be **sorted** by each of them and **filtered**: never played, not played for N days, added in the last N days, bitrate below N kbps.
- Tracks scanned before M5b get their file details at the next scan, without being analyzed again.

### Programming details

What music schedulers sort and separate by, filled in from where each one is reliable:

- **From MusicBrainz**, for identified tracks ([M1b](m1b-library-management.md)): album, record label and catalog number, ISRC, the year of first release, and genres (as suggestions). Fetched for the whole library on request, at MusicBrainz's pace (one request a second), and kept up to date for new tracks.
- **Era**, from the year of first release: the decade (`70s`, `80s`…) by default, or eras the station defines (`classics`, `recurrents`, `currents`).
- **BPM**, measured during the analysis ([Import and analysis](../analysis.md)): MusicBrainz has no tempo. Editable when the measure is wrong (half or double tempo).
- **Mood and sound code**, set by the programmers on scales the station defines (a sound code such as soft, medium, hard; an energy from 1 to 5), on one track or a selection at once. MusicBrainz has no mood, and AcousticBrainz, which estimated them, closed in 2022.
- All of them show in the library and on the track page, sort and filter like the track details, and can be written into the files' tags when the station wants (ISRC, label, BPM, mood).

### Labels

- **Labels** are keywords on tracks (`christmas`, `instrumental`, `live`, `female vocal`), several per track, with a color. They live in Heron only: files are not modified.
- Create, rename, merge and delete labels; add or remove a label on one track or on a whole selection at once.
- The library filters by label (all of these, any of these, none of these) and combines it with the other filters, so a selection can be queued, put in a category or labelled in turn.
- Labels are not categories: clocks call categories. A category can be filled from a label in one action.

### Editing the files' tags

- Edit title, artist, album, album artist, year, genre, track and disc numbers from the browser, on one track or on a selection (a field set on a selection applies to all of it; fields left alone keep each track's value).
- The tags are written by `heron-librarian`, the only service that writes to the library, as a library task: journaled like any batch, undone byte for byte, without analyzing the tracks again. The scanner finds the files unchanged ([Library management](../library.md)).
- Files are not moved: organizing the library ([M1b](m1b-library-management.md)) files them by their new tags, after its dry run.
- Fields Heron does not edit (comments, lyrics, pictures, ReplayGain) are kept as they are.

## Acceptance criteria

- [ ] The library sorts and filters by date added, last played, play count and bitrate; the counts match the play history (skipped plays under the threshold excluded).
- [ ] Identified tracks get their album, label, ISRC, first release year and era from MusicBrainz; BPM is measured within 2 % on test tracks of known tempo; mood and sound code are set on a selection in one action.
- [ ] A label is added to and removed from 1,000 selected tracks in one action, and the library filters by labels combined with its other filters.
- [ ] Tags edited on a selection are written to the files and undone byte for byte; the tracks keep their ID, analysis, categories and history.

## References

- [Web UI](../web.md)
- [Library management](../library.md)
- [Data model](../data-model.md)
- [Rotation](../rotation.md)
- [Open questions](../open-questions.md)
