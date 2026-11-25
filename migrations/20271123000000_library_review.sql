-- SPDX-License-Identifier: AGPL-3.0-or-later
--
-- The library reviewed like the buffer zone (docs/library.md#identification):
-- each track's file is checked for damage, and a track can be searched on
-- MusicBrainz by hand.

-- What the check of a track's file found. Checked again when the file
-- changes (its hash then differs).
CREATE TABLE track_integrity (
    track_id    BIGINT UNSIGNED NOT NULL,
    file_hash   BINARY(32) NULL,
    -- The damage found (`broken_seek_table`, `damaged_audio`,
    -- `mixed_streams`), or NULL for a sound file.
    damage      JSON NULL,
    checked_at  DATETIME(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    PRIMARY KEY (track_id),
    CONSTRAINT track_integrity_track FOREIGN KEY (track_id) REFERENCES tracks (id) ON DELETE CASCADE
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4;

ALTER TABLE library_tasks
    MODIFY COLUMN kind ENUM('organize_plan', 'organize_apply', 'undo', 'review_apply',
                            'artist_relations', 'edit_tags', 'musicbrainz_details',
                            'staging_import', 'delete', 'empty_trash', 'render_medley',
                            'preview_medley', 'fetch_covers', 'apply_cover',
                            'write_tag_overrides', 'review_search') NOT NULL;
