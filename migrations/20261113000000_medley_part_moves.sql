-- SPDX-License-Identifier: AGPL-3.0-or-later
--
-- Medley parts can be moved on the timeline: how far each one was moved
-- from where the recipe puts it. See docs/medleys.md.

ALTER TABLE medley_parts
    -- In ms, negative for earlier.
    ADD COLUMN move_ms INT NOT NULL DEFAULT 0 AFTER shift_ms;
