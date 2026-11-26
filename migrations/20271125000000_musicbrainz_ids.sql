-- SPDX-License-Identifier: AGPL-3.0-or-later
--
-- MusicBrainz identifiers for other systems (docs/api.md#webhooks): the
-- release group of each track, read from its tags, and the Wikidata item of
-- each MusicBrainz artist, looked up with the MusicBrainz details.

ALTER TABLE tracks
    ADD COLUMN musicbrainz_release_group_id CHAR(36) NULL AFTER musicbrainz_release_id;

-- Each MusicBrainz artist the tracks carry. `wikidata_id` is NULL when
-- MusicBrainz has no Wikidata link for the artist.
CREATE TABLE musicbrainz_artists (
    mbid         CHAR(36) NOT NULL,
    wikidata_id  VARCHAR(16) NULL,
    fetched_at   DATETIME(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    PRIMARY KEY (mbid)
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4;
