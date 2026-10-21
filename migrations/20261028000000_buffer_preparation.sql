-- SPDX-License-Identifier: AGPL-3.0-or-later
--
-- M5e: files of the buffer zone are prepared before they are imported:
-- their tags edited, and the categories, labels, mood and sound code the
-- track an import creates gets in the step that creates it. See
-- docs/library.md.

-- Tags are edited by hand on top of those the file gets from its
-- identification, a release picked by hand or its own tags (`base_tags`):
-- `applied_tags` is `base_tags` with `tag_edits` (field: value, or null to
-- remove it). An edit is applied without looking the release up again.
ALTER TABLE staged_files
    ADD COLUMN base_tags   JSON NULL AFTER choice,
    ADD COLUMN tag_edits   JSON NULL AFTER base_tags,
    ADD COLUMN mood        VARCHAR(64) NULL AFTER tag_edits,
    ADD COLUMN sound_code  VARCHAR(64) NULL AFTER mood;

UPDATE staged_files SET base_tags = applied_tags WHERE applied_tags IS NOT NULL;

-- Deleting a category or a label takes it off the files too.
CREATE TABLE staged_file_categories (
    staged_file_id  BIGINT UNSIGNED NOT NULL,
    category_id     BIGINT UNSIGNED NOT NULL,
    weight          DOUBLE NOT NULL DEFAULT 1.0,
    PRIMARY KEY (staged_file_id, category_id),
    KEY staged_file_categories_category (category_id),
    CONSTRAINT staged_file_categories_file FOREIGN KEY (staged_file_id) REFERENCES staged_files (id) ON DELETE CASCADE,
    CONSTRAINT staged_file_categories_category FOREIGN KEY (category_id) REFERENCES categories (id) ON DELETE CASCADE
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4;

CREATE TABLE staged_file_labels (
    staged_file_id  BIGINT UNSIGNED NOT NULL,
    label_id        BIGINT UNSIGNED NOT NULL,
    PRIMARY KEY (staged_file_id, label_id),
    KEY staged_file_labels_label (label_id),
    CONSTRAINT staged_file_labels_file FOREIGN KEY (staged_file_id) REFERENCES staged_files (id) ON DELETE CASCADE,
    CONSTRAINT staged_file_labels_label FOREIGN KEY (label_id) REFERENCES labels (id) ON DELETE CASCADE
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4;
