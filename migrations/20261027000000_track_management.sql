-- SPDX-License-Identifier: AGPL-3.0-or-later
--
-- M5c: track management. New files wait in a buffer zone (uploads and the
-- incoming folder) until someone imports them; tracks are deleted through
-- a trash they can be restored from until it is purged. See
-- docs/library.md.

-- In the trash since then (its file is under `.heron-trash/` in the
-- library, or it had no file). Deleted tracks are also marked missing, so
-- nothing plays them; the trash is purged after `trash_days`.
ALTER TABLE tracks
    ADD COLUMN deleted_at DATETIME(6) NULL AFTER missing_since,
    ADD KEY tracks_deleted_at (deleted_at);

-- `delete`: tracks moved to the trash (undone to restore them).
-- `purge`: tracks removed from the trash for good.
ALTER TABLE library_batches
    MODIFY COLUMN kind ENUM('import', 'organize', 'undo', 'review', 'edit', 'delete', 'purge') NOT NULL;

-- Imports from the buffer zone. Moves to and from the trash stay in the
-- library (`.heron-trash/` is at its root).
ALTER TABLE library_moves
    MODIFY COLUMN from_root ENUM('incoming', 'library', 'staging') NOT NULL;

ALTER TABLE library_tasks
    MODIFY COLUMN kind ENUM('organize_plan', 'organize_apply', 'undo', 'review_apply',
                            'artist_relations', 'edit_tags', 'musicbrainz_details',
                            'staging_import', 'delete', 'empty_trash') NOT NULL;

-- Buffer files are compared with the library's fingerprints.
ALTER TABLE track_fingerprints
    ADD COLUMN chromaprint_hash BINARY(16) AS (UNHEX(MD5(chromaprint))) STORED,
    ADD KEY track_fingerprints_hash (chromaprint_hash);

-- The files of one upload, or of one folder of the incoming folder: they
-- are identified together, so the files of an album agree on their release.
CREATE TABLE staged_groups (
    id          BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    source      ENUM('upload', 'incoming') NOT NULL,
    -- The folder's name, or what the uploader called it.
    name        VARCHAR(512) NOT NULL,
    created_by  BIGINT UNSIGNED NULL,
    created_at  DATETIME(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    PRIMARY KEY (id),
    CONSTRAINT staged_groups_user FOREIGN KEY (created_by) REFERENCES users (id) ON DELETE SET NULL
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4;

-- A file in the buffer zone (HERON_STAGING_FOLDER), written there by
-- heron-api (uploads) or moved there by heron-librarian (incoming folder).
--   receiving:   being uploaded or moved in; never shown as ready
--   identifying: complete, waiting for the librarian (identification, or
--                the tags and path of another choice)
--   ready:       identified and checked, waiting for someone to import it
--   importing, imported (track_id), discarded (the file is removed), failed
CREATE TABLE staged_files (
    id                      BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    group_id                BIGINT UNSIGNED NOT NULL,
    -- Relative to the buffer folder, `/` separators.
    staged_path             VARCHAR(4096) CHARACTER SET utf8mb4 COLLATE utf8mb4_bin NOT NULL,
    -- Its path in the upload or in the incoming folder.
    original_name           VARCHAR(4096) NOT NULL,
    size                    BIGINT UNSIGNED NOT NULL,
    content_hash            BINARY(32) NULL,
    status                  ENUM('receiving', 'identifying', 'ready', 'importing', 'imported',
                                 'discarded', 'failed') NOT NULL DEFAULT 'receiving',
    -- The file's own tags, duration and embedded cover.
    tags                    JSON NULL,
    duration_ms             INT UNSIGNED NULL,
    has_cover               BOOLEAN NOT NULL DEFAULT FALSE,
    chromaprint             BLOB NULL,
    -- Outcome of the identification: status, reason, score, recording and
    -- release MBIDs, candidates.
    identification          JSON NULL,
    -- What to import it as: another candidate release, or its own tags.
    -- NULL: the identified release, else its own tags.
    choice                  JSON NULL,
    -- The tags written on import, and where the layout files it.
    applied_tags            JSON NULL,
    planned_path            VARCHAR(4096) NULL,
    -- Why it should be looked at: duplicate, undecodable, too_short, …
    flags                   JSON NULL,
    duplicate_of_track_id   BIGINT UNSIGNED NULL,
    duplicate_of_staged_id  BIGINT UNSIGNED NULL,
    -- The track it became.
    track_id                BIGINT UNSIGNED NULL,
    error                   TEXT NULL,
    uploaded_by             BIGINT UNSIGNED NULL,
    created_at              DATETIME(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    updated_at              DATETIME(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6) ON UPDATE CURRENT_TIMESTAMP(6),
    PRIMARY KEY (id),
    KEY staged_files_status (status, group_id),
    KEY staged_files_group (group_id),
    KEY staged_files_hash (content_hash),
    CONSTRAINT staged_files_group FOREIGN KEY (group_id) REFERENCES staged_groups (id) ON DELETE CASCADE,
    CONSTRAINT staged_files_duplicate FOREIGN KEY (duplicate_of_track_id) REFERENCES tracks (id) ON DELETE SET NULL,
    CONSTRAINT staged_files_track FOREIGN KEY (track_id) REFERENCES tracks (id) ON DELETE SET NULL,
    CONSTRAINT staged_files_user FOREIGN KEY (uploaded_by) REFERENCES users (id) ON DELETE SET NULL
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4;
