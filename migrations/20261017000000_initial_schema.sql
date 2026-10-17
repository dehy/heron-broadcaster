-- SPDX-License-Identifier: AGPL-3.0-or-later
--
-- Initial schema. See docs/data-model.md.
-- Conventions: UTC timestamps as DATETIME(6), durations in milliseconds,
-- JSON columns for settings whose shape is still evolving.

-- Library ---------------------------------------------------------------

CREATE TABLE tracks (
    id            BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    path          VARCHAR(4096) CHARACTER SET utf8mb4 COLLATE utf8mb4_bin NOT NULL,
    -- Paths can exceed the InnoDB index limit, so uniqueness is enforced on a hash.
    path_hash     BINARY(32) AS (UNHEX(SHA2(path, 256))) STORED,
    file_hash     BINARY(32) NULL,
    file_size     BIGINT UNSIGNED NULL,
    file_mtime    DATETIME(6) NULL,
    title         VARCHAR(512) NULL,
    artist        VARCHAR(512) NULL,
    album         VARCHAR(512) NULL,
    duration_ms   INT UNSIGNED NULL,
    enabled       BOOLEAN NOT NULL DEFAULT TRUE,
    created_at    DATETIME(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    updated_at    DATETIME(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6) ON UPDATE CURRENT_TIMESTAMP(6),
    PRIMARY KEY (id),
    UNIQUE KEY tracks_path_hash (path_hash),
    KEY tracks_artist (artist(191)),
    KEY tracks_album (album(191))
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4;

CREATE TABLE track_analysis (
    track_id              BIGINT UNSIGNED NOT NULL,
    analysis_version      INT UNSIGNED NOT NULL,
    integrated_lufs       DOUBLE NULL,
    true_peak_db          DOUBLE NULL,
    cue_in_ms             INT UNSIGNED NOT NULL,
    cue_out_ms            INT UNSIGNED NOT NULL,
    fade_out_start_ms     INT UNSIGNED NULL,
    has_internal_silence  BOOLEAN NOT NULL DEFAULT FALSE,
    analyzed_at           DATETIME(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    PRIMARY KEY (track_id),
    KEY track_analysis_version (analysis_version),
    CONSTRAINT track_analysis_track FOREIGN KEY (track_id) REFERENCES tracks (id) ON DELETE CASCADE
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4;

-- Every column is an override: NULL means "use the default".
CREATE TABLE track_overrides (
    track_id            BIGINT UNSIGNED NOT NULL,
    cue_in_ms           INT UNSIGNED NULL,
    cue_out_ms          INT UNSIGNED NULL,
    crossfade           JSON NULL,
    gap_killer_enabled  BOOLEAN NULL,
    updated_at          DATETIME(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6) ON UPDATE CURRENT_TIMESTAMP(6),
    PRIMARY KEY (track_id),
    CONSTRAINT track_overrides_track FOREIGN KEY (track_id) REFERENCES tracks (id) ON DELETE CASCADE
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4;

-- Queue between the scanner (heron-api) and heron-analyzer, claimed with
-- SELECT ... FOR UPDATE SKIP LOCKED. See docs/architecture.md.
CREATE TABLE analysis_jobs (
    id                BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    track_id          BIGINT UNSIGNED NOT NULL,
    analysis_version  INT UNSIGNED NOT NULL,
    status            ENUM('pending', 'running', 'done', 'failed') NOT NULL DEFAULT 'pending',
    attempts          INT UNSIGNED NOT NULL DEFAULT 0,
    claimed_by        VARCHAR(255) NULL,
    claimed_at        DATETIME(6) NULL,
    error             TEXT NULL,
    created_at        DATETIME(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    updated_at        DATETIME(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6) ON UPDATE CURRENT_TIMESTAMP(6),
    PRIMARY KEY (id),
    KEY analysis_jobs_status (status, created_at),
    KEY analysis_jobs_track (track_id),
    CONSTRAINT analysis_jobs_track FOREIGN KEY (track_id) REFERENCES tracks (id) ON DELETE CASCADE
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4;

-- Categories ------------------------------------------------------------

CREATE TABLE categories (
    id          BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    name        VARCHAR(255) NOT NULL,
    color       CHAR(7) NULL,
    created_at  DATETIME(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    PRIMARY KEY (id),
    UNIQUE KEY categories_name (name)
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4;

CREATE TABLE track_categories (
    track_id     BIGINT UNSIGNED NOT NULL,
    category_id  BIGINT UNSIGNED NOT NULL,
    weight       DOUBLE NOT NULL DEFAULT 1.0,
    PRIMARY KEY (track_id, category_id),
    KEY track_categories_category (category_id),
    CONSTRAINT track_categories_track FOREIGN KEY (track_id) REFERENCES tracks (id) ON DELETE CASCADE,
    CONSTRAINT track_categories_category FOREIGN KEY (category_id) REFERENCES categories (id) ON DELETE CASCADE
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4;

-- Clockwheels -----------------------------------------------------------

CREATE TABLE clocks (
    id          BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    name        VARCHAR(255) NOT NULL,
    created_at  DATETIME(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    PRIMARY KEY (id),
    UNIQUE KEY clocks_name (name)
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4;

CREATE TABLE clock_slots (
    id                BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    clock_id          BIGINT UNSIGNED NOT NULL,
    position          INT UNSIGNED NOT NULL,
    category_id       BIGINT UNSIGNED NOT NULL,
    selection_method  ENUM('least_recently_played', 'random', 'weighted') NOT NULL DEFAULT 'least_recently_played',
    enforce_rules     BOOLEAN NOT NULL DEFAULT TRUE,
    PRIMARY KEY (id),
    UNIQUE KEY clock_slots_position (clock_id, position),
    KEY clock_slots_category (category_id),
    CONSTRAINT clock_slots_clock FOREIGN KEY (clock_id) REFERENCES clocks (id) ON DELETE CASCADE,
    CONSTRAINT clock_slots_category FOREIGN KEY (category_id) REFERENCES categories (id) ON DELETE RESTRICT
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4;

-- day_of_week follows ISO 8601: 1 = Monday ... 7 = Sunday.
CREATE TABLE clock_schedule (
    day_of_week  TINYINT UNSIGNED NOT NULL,
    hour         TINYINT UNSIGNED NOT NULL,
    clock_id     BIGINT UNSIGNED NOT NULL,
    PRIMARY KEY (day_of_week, hour),
    KEY clock_schedule_clock (clock_id),
    CONSTRAINT clock_schedule_day CHECK (day_of_week BETWEEN 1 AND 7),
    CONSTRAINT clock_schedule_hour CHECK (hour BETWEEN 0 AND 23),
    CONSTRAINT clock_schedule_clock FOREIGN KEY (clock_id) REFERENCES clocks (id) ON DELETE CASCADE
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4;

-- History ---------------------------------------------------------------

-- track_id is NULL for fallback files that are not in the library, and after
-- a track is deleted; artist and title are kept so history stays readable.
CREATE TABLE play_history (
    id                  BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    track_id            BIGINT UNSIGNED NULL,
    artist              VARCHAR(512) NULL,
    title               VARCHAR(512) NULL,
    started_at          DATETIME(6) NOT NULL,
    ended_at            DATETIME(6) NULL,
    deck                ENUM('a', 'b', 'fx') NOT NULL,
    source              ENUM('rotation', 'request', 'fallback', 'manual') NOT NULL,
    relaxation_applied  JSON NULL,
    PRIMARY KEY (id),
    KEY play_history_started_at (started_at),
    KEY play_history_track (track_id, started_at),
    CONSTRAINT play_history_track FOREIGN KEY (track_id) REFERENCES tracks (id) ON DELETE SET NULL
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4;

-- Settings --------------------------------------------------------------

CREATE TABLE settings (
    name        VARCHAR(191) NOT NULL,
    value       JSON NOT NULL,
    updated_at  DATETIME(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6) ON UPDATE CURRENT_TIMESTAMP(6),
    PRIMARY KEY (name)
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4;

-- Encoders and outputs (docs/outputs.md) --------------------------------

CREATE TABLE encoder_profiles (
    id            BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    name          VARCHAR(255) NOT NULL,
    encoder       ENUM('mp3', 'aac', 'opus', 'vorbis', 'external_ffmpeg') NOT NULL,
    bitrate_kbps  SMALLINT UNSIGNED NULL,
    sample_rate   INT UNSIGNED NOT NULL DEFAULT 48000,
    channels      TINYINT UNSIGNED NOT NULL DEFAULT 2,
    options       JSON NULL,
    created_at    DATETIME(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    updated_at    DATETIME(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6) ON UPDATE CURRENT_TIMESTAMP(6),
    PRIMARY KEY (id),
    UNIQUE KEY encoder_profiles_name (name)
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4;

CREATE TABLE outputs (
    id                  BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    name                VARCHAR(255) NOT NULL,
    kind                ENUM('icecast', 'shoutcast_v1', 'shoutcast_v2', 'hls', 'file', 'pipe') NOT NULL,
    encoder_profile_id  BIGINT UNSIGNED NOT NULL,
    config              JSON NOT NULL,
    metadata_enabled    BOOLEAN NOT NULL DEFAULT TRUE,
    enabled             BOOLEAN NOT NULL DEFAULT TRUE,
    created_at          DATETIME(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    updated_at          DATETIME(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6) ON UPDATE CURRENT_TIMESTAMP(6),
    PRIMARY KEY (id),
    UNIQUE KEY outputs_name (name),
    KEY outputs_encoder_profile (encoder_profile_id),
    CONSTRAINT outputs_encoder_profile FOREIGN KEY (encoder_profile_id) REFERENCES encoder_profiles (id) ON DELETE RESTRICT
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4;

-- Webhooks --------------------------------------------------------------

CREATE TABLE webhooks (
    id                BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    name              VARCHAR(255) NOT NULL,
    url               VARCHAR(2048) NOT NULL,
    -- Needed in clear text to compute the HMAC signature.
    secret            VARCHAR(255) NOT NULL,
    events            JSON NOT NULL,
    payload_template  JSON NULL,
    enabled           BOOLEAN NOT NULL DEFAULT TRUE,
    created_at        DATETIME(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    updated_at        DATETIME(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6) ON UPDATE CURRENT_TIMESTAMP(6),
    PRIMARY KEY (id),
    UNIQUE KEY webhooks_name (name)
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4;

CREATE TABLE webhook_deliveries (
    id               BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    webhook_id       BIGINT UNSIGNED NOT NULL,
    event            VARCHAR(64) NOT NULL,
    payload          JSON NOT NULL,
    status           ENUM('pending', 'succeeded', 'failed') NOT NULL DEFAULT 'pending',
    attempts         INT UNSIGNED NOT NULL DEFAULT 0,
    response_status  SMALLINT UNSIGNED NULL,
    error            TEXT NULL,
    next_attempt_at  DATETIME(6) NULL,
    created_at       DATETIME(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    delivered_at     DATETIME(6) NULL,
    PRIMARY KEY (id),
    KEY webhook_deliveries_webhook (webhook_id, created_at),
    KEY webhook_deliveries_due (status, next_attempt_at),
    CONSTRAINT webhook_deliveries_webhook FOREIGN KEY (webhook_id) REFERENCES webhooks (id) ON DELETE CASCADE
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4;

-- Users and access (local authentication only for now) ------------------

CREATE TABLE users (
    id             BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    username       VARCHAR(191) NOT NULL,
    password_hash  VARCHAR(255) NOT NULL,
    role           ENUM('admin', 'operator', 'viewer') NOT NULL DEFAULT 'viewer',
    enabled        BOOLEAN NOT NULL DEFAULT TRUE,
    last_login_at  DATETIME(6) NULL,
    created_at     DATETIME(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    updated_at     DATETIME(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6) ON UPDATE CURRENT_TIMESTAMP(6),
    PRIMARY KEY (id),
    UNIQUE KEY users_username (username)
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4;

-- Only hashes of session and API tokens are stored.
CREATE TABLE sessions (
    token_hash    BINARY(32) NOT NULL,
    user_id       BIGINT UNSIGNED NOT NULL,
    created_at    DATETIME(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    last_seen_at  DATETIME(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    expires_at    DATETIME(6) NOT NULL,
    PRIMARY KEY (token_hash),
    KEY sessions_user (user_id),
    KEY sessions_expires_at (expires_at),
    CONSTRAINT sessions_user FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4;

CREATE TABLE api_tokens (
    id            BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    user_id       BIGINT UNSIGNED NOT NULL,
    name          VARCHAR(255) NOT NULL,
    token_hash    BINARY(32) NOT NULL,
    created_at    DATETIME(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    last_used_at  DATETIME(6) NULL,
    expires_at    DATETIME(6) NULL,
    revoked_at    DATETIME(6) NULL,
    PRIMARY KEY (id),
    UNIQUE KEY api_tokens_token_hash (token_hash),
    KEY api_tokens_user (user_id),
    CONSTRAINT api_tokens_user FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4;
