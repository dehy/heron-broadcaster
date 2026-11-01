-- SPDX-License-Identifier: AGPL-3.0-or-later
--
-- Each track's lowest and highest peak between its cue points, in dBFS
-- (docs/analysis.md#job-queue), computed by heron-api from the waveform.
-- The cue points and analysis time they come from tell when to compute
-- them again.

CREATE TABLE track_levels (
    track_id     BIGINT UNSIGNED NOT NULL,
    min_db       DECIMAL(4, 1) NOT NULL,
    max_db       DECIMAL(4, 1) NOT NULL,
    cue_in_ms    INT UNSIGNED NOT NULL,
    cue_out_ms   INT UNSIGNED NOT NULL,
    analyzed_at  DATETIME(6) NOT NULL,
    PRIMARY KEY (track_id),
    KEY track_levels_min (min_db),
    KEY track_levels_max (max_db),
    CONSTRAINT track_levels_track FOREIGN KEY (track_id) REFERENCES tracks (id) ON DELETE CASCADE
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4;
