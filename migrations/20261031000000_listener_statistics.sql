-- SPDX-License-Identifier: AGPL-3.0-or-later
--
-- Listener statistics, read from the stream servers by heron-api once a
-- minute. No IP address is stored: listeners are told apart within a day
-- by a keyed hash of their address and player. See docs/statistics.md.

-- The stream server's admin credentials (admin_username, admin_password),
-- for its listener list. Never pushed to the engine.
ALTER TABLE outputs
    ADD COLUMN statistics JSON NULL AFTER config;

-- Listeners of each output, minute by minute. A minute the server could
-- not be read has no row.
CREATE TABLE listener_samples (
    output_id     BIGINT UNSIGNED NOT NULL,
    -- The start of the minute.
    sampled_at    DATETIME NOT NULL,
    listeners     INT UNSIGNED NOT NULL,
    -- Whether the listener list was read too (sessions are up to date).
    sessions      BOOLEAN NOT NULL,
    PRIMARY KEY (output_id, sampled_at),
    KEY listener_samples_at (sampled_at),
    CONSTRAINT listener_samples_output FOREIGN KEY (output_id) REFERENCES outputs (id) ON DELETE CASCADE
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4;

CREATE TABLE listener_sessions (
    id             BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    output_id      BIGINT UNSIGNED NOT NULL,
    -- BLAKE3 of the address and player, keyed with the day's key.
    listener_key   BINARY(32) NOT NULL,
    user_agent     VARCHAR(512) NOT NULL,
    -- The server's ID for the connection.
    client_id      BIGINT UNSIGNED NOT NULL,
    -- Station time: sessions are split at midnight.
    day            DATE NOT NULL,
    started_at     DATETIME(3) NOT NULL,
    last_seen_at   DATETIME(3) NOT NULL,
    -- NULL while connected; the last time the listener was seen.
    ended_at       DATETIME(3) NULL,
    -- Ended at midnight with the listener still connected: not a departure.
    split          BOOLEAN NOT NULL DEFAULT FALSE,
    duration_s     INT UNSIGNED NOT NULL DEFAULT 0,
    PRIMARY KEY (id),
    KEY listener_sessions_open (output_id, ended_at),
    KEY listener_sessions_day (day),
    KEY listener_sessions_started (started_at),
    KEY listener_sessions_ended (ended_at),
    CONSTRAINT listener_sessions_output FOREIGN KEY (output_id) REFERENCES outputs (id) ON DELETE CASCADE
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4;

-- The random key of each day's listener keys, deleted once its day is
-- over so that days cannot be linked.
CREATE TABLE listener_day_keys (
    day   DATE NOT NULL,
    `key` BINARY(32) NOT NULL,
    PRIMARY KEY (day)
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4;

-- Totals per day and per hour (station time), per output and for the whole
-- station (output_id NULL). Kept when sessions and samples are purged.
CREATE TABLE listener_totals (
    output_id           BIGINT UNSIGNED NULL,
    day                 DATE NOT NULL,
    -- 0 to 23; NULL for the whole day.
    hour                TINYINT UNSIGNED NULL,
    -- Minutes measured.
    minutes             INT UNSIGNED NOT NULL,
    peak                INT UNSIGNED NOT NULL,
    average             DOUBLE NOT NULL,
    listening_hours     DOUBLE NOT NULL,
    -- NULL when no listener list was read in the period.
    sessions            INT UNSIGNED NULL,
    session_s           BIGINT UNSIGNED NULL,
    distinct_listeners  INT UNSIGNED NULL,
    output_key          BIGINT UNSIGNED AS (COALESCE(output_id, 0)) STORED,
    hour_key            TINYINT UNSIGNED AS (COALESCE(hour, 255)) STORED,
    UNIQUE KEY listener_totals_row (day, output_key, hour_key),
    KEY listener_totals_output (output_id),
    CONSTRAINT listener_totals_output FOREIGN KEY (output_id) REFERENCES outputs (id) ON DELETE CASCADE
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4;
