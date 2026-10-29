-- SPDX-License-Identifier: AGPL-3.0-or-later
--
-- Liners over the music fall between whole hooks; each one says how its
-- gap is shared between the hook before and the hook after.
-- See docs/medleys.md.

ALTER TABLE medley_parts
    -- From the middle of the gap, in ms; positive for more of the liner
    -- over the hook before.
    ADD COLUMN shift_ms INT NOT NULL DEFAULT 0 AFTER to_ms;
