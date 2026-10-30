-- SPDX-License-Identifier: AGPL-3.0-or-later
--
-- Several stations in one Heron (docs/stations.md, M13). An existing Heron
-- becomes its first station, with everything it has: its clocks, schedule,
-- selection log, history, outputs, webhooks, listener totals and the
-- per-station settings groups.

CREATE TABLE stations (
    id          BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    -- Empty until heron-api names it, on start, from HERON_STATION_NAME.
    name        VARCHAR(255) NOT NULL,
    -- Lowercase letters, digits and '-': badges and public routes.
    code        VARCHAR(32) NOT NULL,
    -- The engine's gRPC address; NULL for the first station: HERON_ENGINE_URL.
    engine_url  VARCHAR(2048) NULL,
    enabled     BOOLEAN NOT NULL DEFAULT TRUE,
    position    INT UNSIGNED NOT NULL DEFAULT 0,
    created_at  DATETIME(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    updated_at  DATETIME(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6) ON UPDATE CURRENT_TIMESTAMP(6),
    PRIMARY KEY (id),
    UNIQUE KEY stations_code (code)
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4;

INSERT INTO stations (id, name, code, engine_url, enabled, position)
VALUES (1, '', 'main', NULL, TRUE, 0);

-- Per-station settings groups: station, playout, eq, queue, engine, rotation.
CREATE TABLE station_settings (
    station_id  BIGINT UNSIGNED NOT NULL,
    name        VARCHAR(191) NOT NULL,
    value       JSON NOT NULL,
    updated_at  DATETIME(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6) ON UPDATE CURRENT_TIMESTAMP(6),
    PRIMARY KEY (station_id, name),
    CONSTRAINT station_settings_station FOREIGN KEY (station_id) REFERENCES stations (id) ON DELETE CASCADE
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4;

INSERT INTO station_settings (station_id, name, value)
SELECT 1, name, value FROM settings
WHERE name IN ('station', 'playout', 'eq', 'queue', 'engine', 'rotation');

DELETE FROM settings
WHERE name IN ('station', 'playout', 'eq', 'queue', 'engine', 'rotation');

-- Clocks and the schedule ------------------------------------------------

ALTER TABLE clocks
    ADD COLUMN station_id BIGINT UNSIGNED NOT NULL DEFAULT 1 AFTER id,
    DROP KEY clocks_name,
    ADD UNIQUE KEY clocks_name (station_id, name),
    ADD CONSTRAINT clocks_station FOREIGN KEY (station_id) REFERENCES stations (id) ON DELETE CASCADE;
ALTER TABLE clocks ALTER COLUMN station_id DROP DEFAULT;

ALTER TABLE clock_schedule
    ADD COLUMN station_id BIGINT UNSIGNED NOT NULL DEFAULT 1 FIRST,
    DROP PRIMARY KEY,
    ADD PRIMARY KEY (station_id, day_of_week, hour),
    ADD CONSTRAINT clock_schedule_station FOREIGN KEY (station_id) REFERENCES stations (id) ON DELETE CASCADE;
ALTER TABLE clock_schedule ALTER COLUMN station_id DROP DEFAULT;

-- History and the selection log -----------------------------------------

ALTER TABLE play_history
    ADD COLUMN station_id BIGINT UNSIGNED NOT NULL DEFAULT 1 AFTER id,
    ADD KEY play_history_station (station_id, started_at),
    ADD CONSTRAINT play_history_station FOREIGN KEY (station_id) REFERENCES stations (id) ON DELETE CASCADE;
ALTER TABLE play_history ALTER COLUMN station_id DROP DEFAULT;

ALTER TABLE rotation_picks
    ADD COLUMN station_id BIGINT UNSIGNED NOT NULL DEFAULT 1 AFTER id,
    DROP KEY rotation_picks_planned,
    DROP KEY rotation_picks_entry,
    ADD KEY rotation_picks_planned (station_id, planned_at),
    ADD KEY rotation_picks_entry (station_id, engine_started_at, entry_id),
    ADD CONSTRAINT rotation_picks_station FOREIGN KEY (station_id) REFERENCES stations (id) ON DELETE CASCADE;
ALTER TABLE rotation_picks ALTER COLUMN station_id DROP DEFAULT;

-- Outputs, listener totals and webhooks ----------------------------------

ALTER TABLE outputs
    ADD COLUMN station_id BIGINT UNSIGNED NOT NULL DEFAULT 1 AFTER id,
    DROP KEY outputs_name,
    ADD UNIQUE KEY outputs_name (station_id, name),
    ADD CONSTRAINT outputs_station FOREIGN KEY (station_id) REFERENCES stations (id) ON DELETE CASCADE;
ALTER TABLE outputs ALTER COLUMN station_id DROP DEFAULT;

-- A row with no output is the whole station's.
ALTER TABLE listener_totals
    ADD COLUMN station_id BIGINT UNSIGNED NOT NULL DEFAULT 1 FIRST,
    DROP KEY listener_totals_row,
    ADD UNIQUE KEY listener_totals_row (station_id, day, output_key, hour_key),
    ADD CONSTRAINT listener_totals_station FOREIGN KEY (station_id) REFERENCES stations (id) ON DELETE CASCADE;
ALTER TABLE listener_totals ALTER COLUMN station_id DROP DEFAULT;

ALTER TABLE webhooks
    ADD COLUMN station_id BIGINT UNSIGNED NOT NULL DEFAULT 1 AFTER id,
    DROP KEY webhooks_name,
    ADD UNIQUE KEY webhooks_name (station_id, name),
    ADD CONSTRAINT webhooks_station FOREIGN KEY (station_id) REFERENCES stations (id) ON DELETE CASCADE;
ALTER TABLE webhooks ALTER COLUMN station_id DROP DEFAULT;
