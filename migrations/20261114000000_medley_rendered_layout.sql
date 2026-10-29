-- SPDX-License-Identifier: AGPL-3.0-or-later
--
-- The layout a medley was rendered with (heron_core::medley::Layout, JSON),
-- so its timeline is its file's whatever changed since. See docs/medleys.md.

ALTER TABLE medleys
    ADD COLUMN rendered_layout JSON NULL AFTER task_id;
