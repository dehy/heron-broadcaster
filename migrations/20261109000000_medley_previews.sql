-- SPDX-License-Identifier: AGPL-3.0-or-later
--
-- Medley previews: the version being edited mixed outside the library.
-- See docs/medleys.md.

ALTER TABLE library_tasks
    MODIFY COLUMN kind ENUM('organize_plan', 'organize_apply', 'undo', 'review_apply',
                            'artist_relations', 'edit_tags', 'musicbrainz_details',
                            'staging_import', 'delete', 'empty_trash', 'render_medley',
                            'preview_medley') NOT NULL;
