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

-- A few starting points, gentler than the classic graphic EQ presets:
-- there is no limiter before M10, and big boosts clip loud tracks.
INSERT INTO eq_presets (name, gains_db) VALUES
    ('Flat',       '[0, 0, 0, 0, 0, 0, 0, 0, 0, 0]'),
    ('Rock',       '[5, 3, -1.5, -3, -1.5, 1.5, 3.5, 4.5, 4.5, 4]'),
    ('Pop',        '[-1, 2, 3.5, 4, 2.5, 0, -1, -1.5, -1.5, -1.5]'),
    ('Soft rock',  '[2.5, 1, 0, -1.5, -1, 0.5, 2, 3, 3, 3]'),
    ('Dance',      '[6, 4.5, 1, 0, 0, -1, -1.5, 2, 3, 3]'),
    ('Voice (FX)', '[-8, -4, 0, 1.5, 2.5, 3, 2, 0, -1, -2]');
