-- SPDX-License-Identifier: AGPL-3.0-or-later
--
-- M1b: library management (incoming folder, identification, tags, moves).
-- See docs/library.md.

-- Chromaprint of the first two minutes (compressed), looked up on AcoustID.
-- Computed by the analyzer, and by the librarian for incoming files.
CREATE TABLE track_fingerprints (
    track_id     BIGINT UNSIGNED NOT NULL,
    chromaprint  BLOB NOT NULL,
    created_at   DATETIME(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    PRIMARY KEY (track_id),
    CONSTRAINT track_fingerprints_track FOREIGN KEY (track_id) REFERENCES tracks (id) ON DELETE CASCADE
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4;

-- Outcome of the identification of a track. Tracks without a row were never
-- identified (imported before M1b, or with identification turned off).
CREATE TABLE track_identifications (
    track_id        BIGINT UNSIGNED NOT NULL,
    status          ENUM('identified', 'needs_review') NOT NULL,
    -- Why the track needs review, for people.
    reason          VARCHAR(255) NULL,
    score           DOUBLE NULL,
    recording_mbid  CHAR(36) NULL,
    release_mbid    CHAR(36) NULL,
    -- Releases that matched, for the review screen.
    candidates      JSON NULL,
    updated_at      DATETIME(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6) ON UPDATE CURRENT_TIMESTAMP(6),
    PRIMARY KEY (track_id),
    KEY track_identifications_status (status),
    CONSTRAINT track_identifications_track FOREIGN KEY (track_id) REFERENCES tracks (id) ON DELETE CASCADE
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4;

-- A group of file operations, undone together.
CREATE TABLE library_batches (
    id           BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    kind         ENUM('import', 'organize', 'undo') NOT NULL,
    -- For an undo, the batch it undoes.
    undoes       BIGINT UNSIGNED NULL,
    created_at   DATETIME(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    finished_at  DATETIME(6) NULL,
    PRIMARY KEY (id),
    CONSTRAINT library_batches_undoes FOREIGN KEY (undoes) REFERENCES library_batches (id)
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4;

-- Journal of every file the librarian moves or re-tags, written before the
-- file is touched, so an interrupted operation is finished or rolled back
-- on restart. from_path = to_path for a re-tag in place.
--   pending:   recorded, the file may or may not have moved yet
--   moved:     the track points at to_path; a copied original still has to
--              be removed
--   done, abandoned (nothing changed), undone (reverted by a later batch)
CREATE TABLE library_moves (
    id            BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    batch_id      BIGINT UNSIGNED NOT NULL,
    track_id      BIGINT UNSIGNED NULL,
    from_root     ENUM('incoming', 'library') NOT NULL,
    from_path     VARCHAR(4096) CHARACTER SET utf8mb4 COLLATE utf8mb4_bin NOT NULL,
    -- Always in the library.
    to_path       VARCHAR(4096) CHARACTER SET utf8mb4 COLLATE utf8mb4_bin NOT NULL,
    method        ENUM('rename', 'copy') NOT NULL,
    -- Hidden file next to to_path that a copy is written to first.
    temp_path     VARCHAR(4096) CHARACTER SET utf8mb4 COLLATE utf8mb4_bin NULL,
    -- BLAKE3 of the whole file written at to_path (copies only).
    content_hash  BINARY(32) NULL,
    status        ENUM('pending', 'moved', 'done', 'abandoned', 'undone') NOT NULL DEFAULT 'pending',
    error         TEXT NULL,
    created_at    DATETIME(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    updated_at    DATETIME(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6) ON UPDATE CURRENT_TIMESTAMP(6),
    PRIMARY KEY (id),
    KEY library_moves_status (status),
    KEY library_moves_batch (batch_id),
    KEY library_moves_track (track_id),
    CONSTRAINT library_moves_batch FOREIGN KEY (batch_id) REFERENCES library_batches (id),
    CONSTRAINT library_moves_track FOREIGN KEY (track_id) REFERENCES tracks (id) ON DELETE SET NULL
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4;

-- Tags written by the librarian, with the tags they replaced, so a change
-- can be undone. Only applies once its move is moved or done.
CREATE TABLE track_tag_changes (
    id              BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    track_id        BIGINT UNSIGNED NOT NULL,
    move_id         BIGINT UNSIGNED NOT NULL,
    previous        JSON NOT NULL,
    applied         JSON NOT NULL,
    cover_embedded  BOOLEAN NOT NULL DEFAULT FALSE,
    created_at      DATETIME(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    -- Set when a later batch wrote the previous tags back.
    undone_at       DATETIME(6) NULL,
    PRIMARY KEY (id),
    UNIQUE KEY track_tag_changes_move (move_id),
    KEY track_tag_changes_track (track_id),
    CONSTRAINT track_tag_changes_track FOREIGN KEY (track_id) REFERENCES tracks (id) ON DELETE CASCADE,
    CONSTRAINT track_tag_changes_move FOREIGN KEY (move_id) REFERENCES library_moves (id) ON DELETE CASCADE
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4;
