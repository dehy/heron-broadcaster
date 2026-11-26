# M21 – Review and MusicBrainz identifiers

**Depends on:** M1b (identification), M5b (MusicBrainz details), M7 (webhooks and the public endpoints)

## Why

Heron identifies its tracks by their audio (AcoustID, then MusicBrainz) and an operator settles what it is not sure of. That is more reliable than anything a website or an app can do from an artist and a title. Yet three things stop Heron from being the single source of its tracks' MusicBrainz identifiers:

- **Reviewing is slow and hard to get right.** An operator picks among editions AcoustID found, one row per release, with no way to search MusicBrainz for a file AcoustID does not know, nor to fix a tag before applying.
- **Only new files are identified.** A library imported before identification existed, or filed without it, stays unidentified unless its files were tagged with Picard.
- **What Heron learns stays inside it.** Its webhooks and public endpoints send the artist, title, album and cover, but no MusicBrainz identifier. Heron keeps the recording, release and first artist, not the release group (the album across its editions), and does not look up the artist's Wikidata item. A station's website that wants the year, the artist's biography or photo, or the album must search MusicBrainz again by text, guess, and keep fixes of its own.

## Scope

### Reviewing

- **A review window**, on Import music and on *To do › To identify*, one file or track after another with keyboard shortcuts: what MusicBrainz found next to the file's own tags, field by field, fixes made in place before applying, a preview of the tags a release would give, another album to pick, a search, or the tags given by hand. A track's page reviews its match again, to correct a wrong one.
- **MusicBrainz searched by album:** a search by title and artist gives one row per album (release group), studio albums first, then the earliest, filtered by kind (studio, live, compilation, single or EP, other). Heron picks the edition the tags come from. Edition notes such as "(Remastered 2011)" are left out of the search. A file AcoustID does not know is searched this way by itself, when the station has an AcoustID key.
- **An album given by hand:** the album field of a search takes a MusicBrainz album's (or edition's) address or ID: only that album is searched, whatever its kind, and its tracklist is shown when no track is titled like the file.
- **A refusal that will not change** (an invalid AcoustID key) sends the files to review at once, with the reason, instead of being tried again for minutes.

### The library

- **Identified in the background:** tracks never identified are identified a few at a time, by their fingerprint or a search, and wait on *To identify*. Nothing is written into the files without a review. Tracks kept as not on MusicBrainz are searched again every 3 months. `identify_library` turns it off.
- **Damaged files:** MP3 files with a broken seek table, damaged frames or several streams glued together are found, in the library (*To do › Damaged files*, the track's page) and in the buffer zone.

### Sent to other systems

- **Stored:** the release group of each track (`tracks.musicbrainz_release_group_id`), read from the tags by the scanner and so written by every import and review; the Wikidata item of each MusicBrainz artist (`musicbrainz_artists`), looked up once with the MusicBrainz details (`artist/{mbid}?inc=url-rels`), again when every track is asked for.
- **Webhooks:** `track` (and each upcoming track of `queue.changed`) has `musicbrainz`: `recording_id`, `release_id`, `release_group_id`, `artist_id`, `artist_wikidata_id`, `first_release_year`. Each is `null` when unknown; `musicbrainz` is `null` when the track has no MusicBrainz ID. New placeholders: `{{musicbrainz.recording_id}}` and the others.
- **Public endpoints and the gateway:** the same `musicbrainz` on `now`, on each upcoming track and on each play of the history: MusicBrainz's public identifiers, still no Heron ID and no path.
- **The track's page** links its album on MusicBrainz and its artist on Wikidata.

## Out of scope

- Several artists per track: the first credited artist only, as Heron stores it.
- Biographies, photos or anything looked up on Wikidata or Wikipedia: Heron gives the item, the receiver does the rest.
- Writing the Wikidata item into the files.
- A webhook event when a track's identifiers change: they come with the track's next play.
- AcoustID's user key, required from 2027 ([#72](https://github.com/dehy/heron-broadcaster/issues/72)).

## Deliverables

- Migrations: `look_again`, `library_review`, `review_preview` (the review, the library's identification and damage check), `musicbrainz_ids` (the release group and the artists' Wikidata items).
- Library: the scanner reads the release group (details version 3).
- Librarian: the review search and preview tasks, the album search by kind, the background identification and the look-again, the damage check, the artists' Wikidata lookup.
- API: the review, preview, search and damage routes; `musicbrainz` on the webhook payload and the public endpoints, the new placeholders; the release group and Wikidata item on `GET /tracks/{id}`.
- Web: the review window on Import music and *To do*, the kind filter, *Damaged files*, the links on the track page. All four catalogs.
- Docs: [Library management](../library.md#identification), [API – Webhooks](../api.md#webhooks), [API – Public endpoints](../api.md#public-endpoints), [Data model](../data-model.md), [Web UI](../web.md#to-do).

## Acceptance criteria

- [x] A search gives one candidate per album, studio albums first, the kinds asked for in the query, the album tag first, every kind when nothing is found (mock MusicBrainz).
- [x] An album given by its address or ID is the only one searched, whatever its kind; an edition given is kept; its tracklist is shown when the title is not on it.
- [x] A match is previewed, fixed and applied; a track identified already is reviewed again.
- [x] Tracks never identified are identified in the background; tracks not on MusicBrainz are looked up again after 3 months.
- [x] An invalid AcoustID key sends the files to review at once.
- [x] A track imported from the buffer zone gets the release group of its release.
- [x] Every artist ID of the library gets its Wikidata item, or none when MusicBrainz has no link; an ID that is not a MusicBrainz ID is never sent; asking for every track looks them up again (mock MusicBrainz).
- [x] Track deliveries carry the six MusicBrainz values of an identified track, `null` for an unidentified one; `{{musicbrainz.release_group_id}}` writes it out; unknown `musicbrainz.*` placeholders are refused.
- [x] `now-playing`, `queue` and `history` carry the same `musicbrainz`, and still no Heron ID.
- [x] All four catalogs.
