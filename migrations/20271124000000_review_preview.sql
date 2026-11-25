-- SPDX-License-Identifier: AGPL-3.0-or-later
--
-- The tags a release would give a library track, shown in its review
-- before they are written (docs/library.md#identification).

ALTER TABLE library_tasks
    MODIFY COLUMN kind ENUM('organize_plan', 'organize_apply', 'undo', 'review_apply',
                            'artist_relations', 'edit_tags', 'musicbrainz_details',
                            'staging_import', 'delete', 'empty_trash', 'render_medley',
                            'preview_medley', 'fetch_covers', 'apply_cover',
                            'write_tag_overrides', 'review_search', 'review_preview') NOT NULL;
