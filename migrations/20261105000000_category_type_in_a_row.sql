-- SPDX-License-Identifier: AGPL-3.0-or-later
--
-- Types that never play twice in a row: rotation never picks a track of
-- such a type right after one, and a clock slot calling it there is passed
-- over. On for Jingle. See docs/rotation.md#never-two-in-a-row.

ALTER TABLE category_types
    ADD COLUMN no_two_in_a_row BOOLEAN NOT NULL DEFAULT FALSE AFTER name;

UPDATE category_types SET no_two_in_a_row = TRUE WHERE name = 'Jingle';
