-- SPDX-License-Identifier: AGPL-3.0-or-later
--
-- Several schedules per station (docs/rotation.md#scheduling, M14). Each
-- station's schedule becomes its default schedule; other schedules play
-- during their periods.

CREATE TABLE schedules (
    id          BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    station_id  BIGINT UNSIGNED NOT NULL,
    -- Empty for the default schedule: the web UI names it.
    name        VARCHAR(255) NOT NULL,
    is_default  BOOLEAN NOT NULL DEFAULT FALSE,
    PRIMARY KEY (id),
    UNIQUE KEY schedules_name (station_id, name),
    CONSTRAINT schedules_station FOREIGN KEY (station_id) REFERENCES stations (id) ON DELETE CASCADE
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4;

-- Station time, on the hour, the end excluded.
CREATE TABLE schedule_periods (
    id           BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    schedule_id  BIGINT UNSIGNED NOT NULL,
    starts_at    DATETIME NOT NULL,
    ends_at      DATETIME NOT NULL,
    -- The years are ignored; the range may run over the new year.
    every_year   BOOLEAN NOT NULL DEFAULT FALSE,
    PRIMARY KEY (id),
    KEY schedule_periods_schedule (schedule_id),
    CONSTRAINT schedule_periods_schedule FOREIGN KEY (schedule_id) REFERENCES schedules (id) ON DELETE CASCADE
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4;

INSERT INTO schedules (station_id, name, is_default)
SELECT id, '', TRUE FROM stations;

ALTER TABLE clock_schedule ADD COLUMN schedule_id BIGINT UNSIGNED NULL FIRST;
UPDATE clock_schedule cs
    JOIN schedules s ON s.station_id = cs.station_id AND s.is_default
    SET cs.schedule_id = s.id;
ALTER TABLE clock_schedule
    DROP FOREIGN KEY clock_schedule_station,
    DROP PRIMARY KEY,
    DROP COLUMN station_id,
    MODIFY schedule_id BIGINT UNSIGNED NOT NULL,
    ADD PRIMARY KEY (schedule_id, day_of_week, hour),
    ADD CONSTRAINT clock_schedule_schedule FOREIGN KEY (schedule_id) REFERENCES schedules (id) ON DELETE CASCADE;
