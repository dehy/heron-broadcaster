-- SPDX-License-Identifier: AGPL-3.0-or-later
--
-- Types whose tracks play on the FX player over the next song's intro,
-- ending where its vocals start (its intro marker, see docs/import.md),
-- when that intro is long enough; otherwise they play on their own, as
-- before. On for Jingle. See docs/audio-engine.md#jingles-over-the-intro.

ALTER TABLE category_types
    ADD COLUMN plays_over_intro BOOLEAN NOT NULL DEFAULT FALSE AFTER sends_metadata;

UPDATE category_types SET plays_over_intro = TRUE WHERE name = 'Jingle';
