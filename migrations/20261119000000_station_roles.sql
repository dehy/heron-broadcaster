-- SPDX-License-Identifier: AGPL-3.0-or-later
--
-- Roles per station (docs/stations.md#roles, M13). users.role is the
-- shared role; a user without a row for a station has `none` on it. Every
-- existing user keeps their role on the first station.

CREATE TABLE user_station_roles (
    user_id     BIGINT UNSIGNED NOT NULL,
    station_id  BIGINT UNSIGNED NOT NULL,
    role        ENUM('none', 'viewer', 'operator', 'admin') NOT NULL,
    PRIMARY KEY (user_id, station_id),
    KEY user_station_roles_station (station_id),
    CONSTRAINT user_station_roles_user FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE,
    CONSTRAINT user_station_roles_station FOREIGN KEY (station_id) REFERENCES stations (id) ON DELETE CASCADE
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4;

INSERT INTO user_station_roles (user_id, station_id, role)
SELECT u.id, (SELECT MIN(id) FROM stations), CAST(u.role AS CHAR) FROM users u;
