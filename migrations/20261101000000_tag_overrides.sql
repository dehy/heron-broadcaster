-- SPDX-License-Identifier: AGPL-3.0-or-later
--
-- Tags edited from the browser while the library setting `write_tags` is
-- off: kept in Heron's database instead of the files. The scanner and the
-- librarian apply them on top of the tags they read from a file, so they
-- survive a rescan or a file changed on disk. See docs/library.md.

CREATE TABLE track_tag_overrides (
    track_id    BIGINT UNSIGNED NOT NULL,
    -- A TagField name: title, artist, album, album_artist, year, genre,
    -- track or disc.
    field       VARCHAR(32) NOT NULL,
    -- The value shown instead of the file's; NULL removes the field.
    value       VARCHAR(512) NULL,
    updated_by  BIGINT UNSIGNED NULL,
    updated_at  DATETIME(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6) ON UPDATE CURRENT_TIMESTAMP(6),
    PRIMARY KEY (track_id, field),
    CONSTRAINT track_tag_overrides_track FOREIGN KEY (track_id) REFERENCES tracks (id) ON DELETE CASCADE,
    CONSTRAINT track_tag_overrides_user FOREIGN KEY (updated_by) REFERENCES users (id) ON DELETE SET NULL
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4;
