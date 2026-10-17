-- SPDX-License-Identifier: AGPL-3.0-or-later
--
-- M5: library work asked through the API (which cannot write files) and
-- carried out by heron-librarian: organize dry runs and applies, undos,
-- and releases picked on the review screen. See docs/library.md.

CREATE TABLE library_tasks (
    id            BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    kind          ENUM('organize_plan', 'organize_apply', 'undo', 'review_apply') NOT NULL,
    params        JSON NOT NULL,
    status        ENUM('pending', 'running', 'done', 'failed') NOT NULL DEFAULT 'pending',
    result        JSON NULL,
    error         TEXT NULL,
    requested_by  BIGINT UNSIGNED NULL,
    created_at    DATETIME(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    started_at    DATETIME(6) NULL,
    finished_at   DATETIME(6) NULL,
    PRIMARY KEY (id),
    KEY library_tasks_status (status, id),
    CONSTRAINT library_tasks_user FOREIGN KEY (requested_by) REFERENCES users (id) ON DELETE SET NULL
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4;

-- `kept`: reviewed, the file keeps its own tags.
ALTER TABLE track_identifications
    MODIFY COLUMN status ENUM('identified', 'needs_review', 'kept') NOT NULL;

-- `review`: a release picked on the review screen, applied to one track.
ALTER TABLE library_batches
    MODIFY COLUMN kind ENUM('import', 'organize', 'undo', 'review') NOT NULL;
