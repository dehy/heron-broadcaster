-- SPDX-License-Identifier: AGPL-3.0-or-later
--
-- Tracks imported as not on MusicBrainz are looked up again every 3 months
-- (docs/library.md#identification): when MusicBrainz knows them by then,
-- they wait on To do › To identify. NULL: 3 months after it was settled.

ALTER TABLE track_identifications
    ADD COLUMN look_again_at DATETIME(6) NULL AFTER candidates;
