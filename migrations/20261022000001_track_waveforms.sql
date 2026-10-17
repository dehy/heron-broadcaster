-- SPDX-License-Identifier: AGPL-3.0-or-later
--
-- M5: overview of each track's level, drawn by the waveform view. Computed
-- with the analysis (heron-analyzer). One byte per `interval_ms`: the
-- highest sample peak in that interval on a dB scale, 0 for -72 dBFS or
-- below, 255 for full scale.

CREATE TABLE track_waveforms (
    track_id     BIGINT UNSIGNED NOT NULL,
    interval_ms  SMALLINT UNSIGNED NOT NULL,
    peaks        MEDIUMBLOB NOT NULL,
    PRIMARY KEY (track_id),
    CONSTRAINT track_waveforms_track FOREIGN KEY (track_id) REFERENCES tracks (id) ON DELETE CASCADE
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4;
