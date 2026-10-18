-- SPDX-License-Identifier: AGPL-3.0-or-later
--
-- M6: clocks that loop across hours, carrying on from their last slot
-- instead of starting over at each hour (SAM Broadcaster's clockwheel).

ALTER TABLE clocks
    ADD COLUMN looping BOOLEAN NOT NULL DEFAULT FALSE AFTER name;
