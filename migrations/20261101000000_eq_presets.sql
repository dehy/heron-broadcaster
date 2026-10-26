-- SPDX-License-Identifier: AGPL-3.0-or-later
--
-- Deck EQ presets. The EQs themselves are the `eq` settings group. See
-- docs/audio-engine.md#deck-eq.

CREATE TABLE eq_presets (
    id          BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    name        VARCHAR(100) NOT NULL,
    -- Ten gains in dB, from 60 Hz up, -12 to +12.
    gains_db    JSON NOT NULL,
    created_at  DATETIME(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    PRIMARY KEY (id),
    UNIQUE KEY eq_presets_name (name)
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_unicode_ci;

INSERT INTO eq_presets (name, gains_db) VALUES ('Flat', '[0, 0, 0, 0, 0, 0, 0, 0, 0, 0]');
