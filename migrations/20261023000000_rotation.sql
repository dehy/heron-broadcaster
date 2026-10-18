-- SPDX-License-Identifier: AGPL-3.0-or-later
--
-- M6: rotation and clockwheels. See docs/rotation.md.

-- Artists ---------------------------------------------------------------
-- Separation compares artists, not names. Names come from the tags (split
-- on "featuring"); a name belongs to one artist, and is moved to another
-- by hand. Similar artists count as the same artist for separation.

CREATE TABLE artists (
    id              BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    name            VARCHAR(512) NOT NULL,
    musicbrainz_id  CHAR(36) NULL,
    created_at      DATETIME(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    PRIMARY KEY (id),
    UNIQUE KEY artists_musicbrainz (musicbrainz_id),
    KEY artists_name (name(191))
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4;

-- `normalized`: lowercase, without accents, spaces collapsed (heron-rotation
-- names::normalize). Unique through a hash: names can exceed the index limit.
CREATE TABLE artist_names (
    id               BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    artist_id        BIGINT UNSIGNED NOT NULL,
    name             VARCHAR(512) NOT NULL,
    normalized       VARCHAR(512) CHARACTER SET utf8mb4 COLLATE utf8mb4_bin NOT NULL,
    normalized_hash  BINARY(32) AS (UNHEX(SHA2(normalized, 256))) STORED,
    -- Moved to its artist by hand, rather than found in the tags.
    by_hand          BOOLEAN NOT NULL DEFAULT FALSE,
    created_at       DATETIME(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    PRIMARY KEY (id),
    UNIQUE KEY artist_names_normalized (normalized_hash),
    KEY artist_names_artist (artist_id),
    CONSTRAINT artist_names_artist FOREIGN KEY (artist_id) REFERENCES artists (id) ON DELETE CASCADE
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4;

-- One row per pair, the smaller ID first.
CREATE TABLE artist_similar (
    artist_id   BIGINT UNSIGNED NOT NULL,
    similar_id  BIGINT UNSIGNED NOT NULL,
    created_at  DATETIME(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    PRIMARY KEY (artist_id, similar_id),
    KEY artist_similar_similar (similar_id),
    CONSTRAINT artist_similar_order CHECK (artist_id < similar_id),
    CONSTRAINT artist_similar_artist FOREIGN KEY (artist_id) REFERENCES artists (id) ON DELETE CASCADE,
    CONSTRAINT artist_similar_other FOREIGN KEY (similar_id) REFERENCES artists (id) ON DELETE CASCADE
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4;

-- Links offered for review: from names (`A & B` when A and B are known
-- artists) and from MusicBrainz relationships. The smaller ID first.
CREATE TABLE artist_suggestions (
    id          BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    artist_id   BIGINT UNSIGNED NOT NULL,
    other_id    BIGINT UNSIGNED NOT NULL,
    source      ENUM('names', 'musicbrainz') NOT NULL,
    reason      VARCHAR(255) NOT NULL,
    status      ENUM('pending', 'accepted', 'dismissed') NOT NULL DEFAULT 'pending',
    created_at  DATETIME(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    PRIMARY KEY (id),
    UNIQUE KEY artist_suggestions_pair (artist_id, other_id),
    KEY artist_suggestions_status (status),
    CONSTRAINT artist_suggestions_order CHECK (artist_id < other_id),
    CONSTRAINT artist_suggestions_artist FOREIGN KEY (artist_id) REFERENCES artists (id) ON DELETE CASCADE,
    CONSTRAINT artist_suggestions_other FOREIGN KEY (other_id) REFERENCES artists (id) ON DELETE CASCADE
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4;

CREATE TABLE track_artists (
    track_id   BIGINT UNSIGNED NOT NULL,
    artist_id  BIGINT UNSIGNED NOT NULL,
    position   TINYINT UNSIGNED NOT NULL,
    PRIMARY KEY (track_id, artist_id),
    KEY track_artists_artist (artist_id),
    CONSTRAINT track_artists_track FOREIGN KEY (track_id) REFERENCES tracks (id) ON DELETE CASCADE,
    CONSTRAINT track_artists_artist FOREIGN KEY (artist_id) REFERENCES artists (id) ON DELETE CASCADE
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4;

-- `artists_key`: hash of the artist and title tags the track's artists were
-- linked from, so tracks whose tags changed are linked again.
ALTER TABLE tracks
    ADD COLUMN musicbrainz_artist_id CHAR(36) NULL AFTER album,
    ADD COLUMN artists_key BINARY(32) NULL AFTER musicbrainz_artist_id;

-- Restrictions ----------------------------------------------------------
-- Hard rules, never relaxed. `rule` is a heron-rotation Rule (JSON,
-- tagged by kind). Targets are not foreign keys: rules on deleted targets
-- are removed with them, or ignored.

CREATE TABLE restrictions (
    id          BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    target      ENUM('track', 'artist', 'category') NOT NULL,
    target_id   BIGINT UNSIGNED NOT NULL,
    rule        JSON NOT NULL,
    enabled     BOOLEAN NOT NULL DEFAULT TRUE,
    note        VARCHAR(512) NULL,
    created_at  DATETIME(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    updated_at  DATETIME(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6) ON UPDATE CURRENT_TIMESTAMP(6),
    PRIMARY KEY (id),
    KEY restrictions_target (target, target_id)
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4;

-- Separation overrides --------------------------------------------------
-- heron-rotation SeparationOverrides (JSON): windows in minutes, missing
-- ones inherited from the station's rotation settings.

ALTER TABLE categories
    ADD COLUMN separation JSON NULL AFTER color;

ALTER TABLE clock_slots
    ADD COLUMN separation JSON NULL AFTER enforce_rules;

-- Selection log ---------------------------------------------------------
-- One row per track rotation queued, with why it was picked. `entry_id` is
-- the engine's queue entry (unique while the engine started at
-- `engine_started_at` runs); the play it became is linked when it starts.

CREATE TABLE rotation_picks (
    id                  BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    track_id            BIGINT UNSIGNED NULL,
    planned_at          DATETIME(6) NOT NULL,
    clock_id            BIGINT UNSIGNED NULL,
    slot                INT UNSIGNED NULL,
    hour_start          DATETIME(6) NULL,
    category_id         BIGINT UNSIGNED NULL,
    step                TINYINT UNSIGNED NOT NULL,
    explanation         JSON NOT NULL,
    engine_started_at   BIGINT UNSIGNED NULL,
    entry_id            BIGINT UNSIGNED NULL,
    play_id             BIGINT UNSIGNED NULL,
    created_at          DATETIME(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    PRIMARY KEY (id),
    KEY rotation_picks_planned (planned_at),
    KEY rotation_picks_entry (engine_started_at, entry_id),
    CONSTRAINT rotation_picks_track FOREIGN KEY (track_id) REFERENCES tracks (id) ON DELETE SET NULL,
    CONSTRAINT rotation_picks_play FOREIGN KEY (play_id) REFERENCES play_history (id) ON DELETE SET NULL
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4;

-- Library tasks: MusicBrainz relationships between the station's artists.
ALTER TABLE library_tasks
    MODIFY COLUMN kind ENUM('organize_plan', 'organize_apply', 'undo', 'review_apply', 'artist_relations') NOT NULL;
