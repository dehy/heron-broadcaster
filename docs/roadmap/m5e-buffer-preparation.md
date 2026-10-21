# M5e – Preparing tracks in the buffer zone

**Depends on:** M5c (the buffer zone) and M5b (labels, mood and sound code).

New music is checked in the buffer zone ([M5c](m5c-track-management.md)), but everything a programmer does next happens after the import: find the new tracks in the library, then put them in categories, label them, set their mood and sound code. Feedback from the first station using the buffer: tracks should be prepared **from the buffer page**, so they are ready to air the moment they are imported; and the tracks just imported are hard to find in the library.

## Scope

### Preparing files

- On a file, a selection or a whole group of the buffer, before it is imported:
  - put it in **categories** (with a weight, 1 by default), or take it out;
  - add or remove **labels**;
  - set or clear its **mood** and **sound code** (on the station's scales).
- Files can be prepared from the moment they show in the buffer, while they are identified, and until they are imported; not while they are being imported.
- The buffer shows what each file has been given, and its group sums it up (categories and labels that all, or some, of its files have).
- Preparing a file changes nothing in the library or in the file: it is kept in the database with the file, and dropped if the file is discarded.

### Importing a prepared file

- The track an import creates gets the file's categories, labels, mood and sound code in the same step that creates it: there is no moment, even after a crash, when a track is in the library without its preparation.
- Categories or labels deleted in the meantime are left out; the rest applies.
- Undoing the import removes the tracks with what they were given, like any imported track.

### Finding imported tracks

- The library can be filtered by **import batch**: the tracks an import created.
- When an import ends, the buffer page links to its tracks in the library. The filter shows in the library as a chip that clears it.
- *Date added* on tracks imported from the buffer is the time of the import.

## Acceptance criteria

- [ ] A group of files uploaded to the buffer is put in two categories, given a label and a mood, one of its files taken out of a category, and imported: each track is in the library with exactly what its file was given, as soon as it exists.
- [ ] A file discarded after being prepared leaves nothing behind; a category deleted after a file was put in it is left out at import, the rest applies.
- [ ] Preparing a file while it is being imported is refused; interrupting the librarian while it imports prepared files (chaos test) leaves no track without its preparation.
- [ ] The buffer page links to the tracks of the import that just ended; the library filtered by that batch lists exactly them, and sorted by date added lists them first.

## References

- [Library management](../library.md#the-buffer-zone)
- [Web UI](../web.md)
- [Data model](../data-model.md)
