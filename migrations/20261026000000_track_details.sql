-- SPDX-License-Identifier: AGPL-3.0-or-later
--
-- M5b: track details, programming details, labels and tag editing.
-- See docs/roadmap/m5b-track-details-tags.md.

-- Track details ---------------------------------------------------------
-- The file's properties and the tags shown and edited in the library, read
-- by the scanner (and by the librarian after it writes tags).
-- `details_version`: what the scanner read; tracks scanned before M5b (0)
-- are read again at the next scan, without being analyzed again.
-- The date a track was added to the station is `created_at`: the scanner
-- creates the track when it first finds the file.

ALTER TABLE tracks
    ADD COLUMN album_artist              VARCHAR(512) NULL AFTER album,
    ADD COLUMN genre                     VARCHAR(512) NULL AFTER album_artist,
    ADD COLUMN year                      SMALLINT UNSIGNED NULL AFTER genre,
    ADD COLUMN track_number              SMALLINT UNSIGNED NULL AFTER year,
    ADD COLUMN disc_number               SMALLINT UNSIGNED NULL AFTER track_number,
    ADD COLUMN musicbrainz_recording_id  CHAR(36) NULL AFTER musicbrainz_artist_id,
    ADD COLUMN musicbrainz_release_id    CHAR(36) NULL AFTER musicbrainz_recording_id,
    ADD COLUMN codec                     VARCHAR(32) NULL AFTER duration_ms,
    ADD COLUMN bitrate_kbps              INT UNSIGNED NULL AFTER codec,
    ADD COLUMN bitrate_mode              ENUM('cbr', 'vbr', 'lossless') NULL AFTER bitrate_kbps,
    ADD COLUMN sample_rate               INT UNSIGNED NULL AFTER bitrate_mode,
    ADD COLUMN channels                  TINYINT UNSIGNED NULL AFTER sample_rate,
    ADD COLUMN details_version           TINYINT UNSIGNED NOT NULL DEFAULT 0 AFTER channels,
    ADD KEY tracks_created (created_at),
    ADD KEY tracks_bitrate (bitrate_kbps),
    ADD KEY tracks_release (musicbrainz_release_id);

-- Play counts and last played skip the FX player and plays shorter than
-- the station's minimum: read from this index alone.
ALTER TABLE play_history
    ADD KEY play_history_plays (track_id, deck, started_at, played_ms);

-- Programming details ---------------------------------------------------

-- Measured during the analysis; `NULL` when no tempo was found.
ALTER TABLE track_analysis
    ADD COLUMN bpm DECIMAL(5, 1) NULL AFTER has_internal_silence;

-- From MusicBrainz, for identified tracks: fetched for `release_id` (the
-- release the track was identified on). A row without values: MusicBrainz
-- does not know the release, or does not list the recording on it.
CREATE TABLE track_musicbrainz (
    track_id            BIGINT UNSIGNED NOT NULL,
    release_id          CHAR(36) NOT NULL,
    recording_id        CHAR(36) NOT NULL,
    album               VARCHAR(512) NULL,
    record_label        VARCHAR(512) NULL,
    catalog_number      VARCHAR(255) NULL,
    isrc                CHAR(12) NULL,
    first_release_year  SMALLINT UNSIGNED NULL,
    -- JSON array of genre names, most voted first: suggestions only.
    genres              JSON NULL,
    fetched_at          DATETIME(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    PRIMARY KEY (track_id),
    KEY track_musicbrainz_year (first_release_year),
    CONSTRAINT track_musicbrainz_track FOREIGN KEY (track_id) REFERENCES tracks (id) ON DELETE CASCADE
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4;

-- Set by the programmers. `bpm` replaces the measured tempo when it is
-- wrong (half or double); mood and sound code take values from the scales
-- in the `programming` settings.
CREATE TABLE track_programming (
    track_id    BIGINT UNSIGNED NOT NULL,
    bpm         DECIMAL(5, 1) NULL,
    mood        VARCHAR(64) NULL,
    sound_code  VARCHAR(64) NULL,
    updated_at  DATETIME(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6) ON UPDATE CURRENT_TIMESTAMP(6),
    PRIMARY KEY (track_id),
    KEY track_programming_mood (mood),
    KEY track_programming_sound_code (sound_code),
    CONSTRAINT track_programming_track FOREIGN KEY (track_id) REFERENCES tracks (id) ON DELETE CASCADE
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4;

-- Labels ----------------------------------------------------------------
-- Keywords on tracks, in Heron only: files are not modified. Names are
-- unique ignoring case.

CREATE TABLE labels (
    id          BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    name        VARCHAR(100) NOT NULL,
    color       CHAR(7) NULL,
    created_at  DATETIME(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    PRIMARY KEY (id),
    UNIQUE KEY labels_name (name)
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_unicode_ci;

CREATE TABLE track_labels (
    track_id    BIGINT UNSIGNED NOT NULL,
    label_id    BIGINT UNSIGNED NOT NULL,
    created_at  DATETIME(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    PRIMARY KEY (track_id, label_id),
    KEY track_labels_label (label_id),
    CONSTRAINT track_labels_track FOREIGN KEY (track_id) REFERENCES tracks (id) ON DELETE CASCADE,
    CONSTRAINT track_labels_label FOREIGN KEY (label_id) REFERENCES labels (id) ON DELETE CASCADE
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4;

-- Editing the files' tags -----------------------------------------------
-- `kind`: `tags` for the tags identification writes (heron-library Tags),
-- `fields` for fields edited from the browser (only those fields).
-- `restore`: rebuilds the file as it was before the change, byte for byte,
-- from the file the change wrote (heron-librarian restore::Delta). NULL
-- for changes made before M5b, and files too large: undoing those writes
-- the previous tags back.

ALTER TABLE track_tag_changes
    ADD COLUMN kind ENUM('tags', 'fields') NOT NULL DEFAULT 'tags' AFTER move_id,
    ADD COLUMN restore MEDIUMBLOB NULL AFTER cover_embedded;

-- `edit`: tags edited from the browser.
ALTER TABLE library_batches
    MODIFY COLUMN kind ENUM('import', 'organize', 'undo', 'review', 'edit') NOT NULL;

ALTER TABLE library_tasks
    MODIFY COLUMN kind ENUM('organize_plan', 'organize_apply', 'undo', 'review_apply',
                            'artist_relations', 'edit_tags', 'musicbrainz_details') NOT NULL;
