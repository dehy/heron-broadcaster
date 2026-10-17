-- SPDX-License-Identifier: AGPL-3.0-or-later
--
-- M1: library scanner and analysis. See docs/analysis.md.

-- tracks.path is relative to the library root, with `/` separators.
-- The mtime is stored as microseconds since the Unix epoch: no time zone to get wrong.
ALTER TABLE tracks
    DROP COLUMN file_mtime,
    ADD COLUMN file_mtime_us BIGINT NULL AFTER file_size,
    -- Set when the file disappears from the library. The track keeps its
    -- categories and history, and is not played while missing.
    ADD COLUMN missing_since DATETIME(6) NULL AFTER enabled,
    ADD KEY tracks_missing_since (missing_since),
    ADD KEY tracks_file_hash (file_hash);

-- Fingerprint of the analysis settings used, so changing a setting
-- re-analyzes every track.
ALTER TABLE track_analysis
    ADD COLUMN params_hash BINARY(32) NOT NULL AFTER analysis_version;
