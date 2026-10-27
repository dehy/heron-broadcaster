-- SPDX-License-Identifier: AGPL-3.0-or-later
--
-- Medleys: song hooks and liners mixed into a library file by the
-- librarian, from a recipe. See docs/medleys.md.

CREATE TABLE medleys (
    id            BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    name          VARCHAR(255) NOT NULL,
    -- Where the rendered medley goes.
    category_id   BIGINT UNSIGNED NULL,
    -- The recipe (songs, hooks, liners), JSON: see heron_core::medley::Recipe.
    recipe        JSON NOT NULL,
    -- The file rendered last, relative to the library root, and its track
    -- once the scanner found it.
    path          VARCHAR(1024) NULL,
    track_id      BIGINT UNSIGNED NULL,
    -- The last render asked (a library task).
    task_id       BIGINT UNSIGNED NULL,
    rendered_at   DATETIME(6) NULL,
    created_at    DATETIME(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    updated_at    DATETIME(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6) ON UPDATE CURRENT_TIMESTAMP(6),
    PRIMARY KEY (id),
    UNIQUE KEY medleys_name (name),
    KEY medleys_track (track_id),
    CONSTRAINT medleys_category FOREIGN KEY (category_id) REFERENCES categories (id) ON DELETE SET NULL,
    CONSTRAINT medleys_track FOREIGN KEY (track_id) REFERENCES tracks (id) ON DELETE SET NULL,
    CONSTRAINT medleys_task FOREIGN KEY (task_id) REFERENCES library_tasks (id) ON DELETE SET NULL
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4;

-- The parts of the medley's current version, in order: song hooks and
-- liners, each a span of its track's file.
CREATE TABLE medley_parts (
    medley_id  BIGINT UNSIGNED NOT NULL,
    position   INT UNSIGNED NOT NULL,
    kind       ENUM('hook', 'liner') NOT NULL,
    track_id   BIGINT UNSIGNED NOT NULL,
    from_ms    INT UNSIGNED NOT NULL,
    to_ms      INT UNSIGNED NOT NULL,
    PRIMARY KEY (medley_id, position),
    KEY medley_parts_track (track_id),
    CONSTRAINT medley_parts_medley FOREIGN KEY (medley_id) REFERENCES medleys (id) ON DELETE CASCADE,
    CONSTRAINT medley_parts_track FOREIGN KEY (track_id) REFERENCES tracks (id) ON DELETE CASCADE
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4;

ALTER TABLE library_tasks
    MODIFY COLUMN kind ENUM('organize_plan', 'organize_apply', 'undo', 'review_apply',
                            'artist_relations', 'edit_tags', 'musicbrainz_details',
                            'staging_import', 'delete', 'empty_trash', 'render_medley') NOT NULL;
