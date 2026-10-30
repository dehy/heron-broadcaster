-- SPDX-License-Identifier: AGPL-3.0-or-later
--
-- Categories, medleys and restrictions tagged with stations
-- (docs/stations.md, M13). Every existing category and medley is the first
-- station's; restrictions without stations apply to every station. The
-- medleys' category and liner categories become the first station's
-- `medley_categories` settings; naming and defaults stay shared.

CREATE TABLE category_stations (
    category_id  BIGINT UNSIGNED NOT NULL,
    station_id   BIGINT UNSIGNED NOT NULL,
    PRIMARY KEY (category_id, station_id),
    KEY category_stations_station (station_id),
    CONSTRAINT category_stations_category FOREIGN KEY (category_id) REFERENCES categories (id) ON DELETE CASCADE,
    CONSTRAINT category_stations_station FOREIGN KEY (station_id) REFERENCES stations (id) ON DELETE CASCADE
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4;

INSERT INTO category_stations (category_id, station_id)
SELECT id, (SELECT MIN(id) FROM stations) FROM categories;

-- position 0: the medley's first station, whose liners it says.
CREATE TABLE medley_stations (
    medley_id   BIGINT UNSIGNED NOT NULL,
    station_id  BIGINT UNSIGNED NOT NULL,
    position    INT UNSIGNED NOT NULL,
    PRIMARY KEY (medley_id, station_id),
    KEY medley_stations_station (station_id),
    CONSTRAINT medley_stations_medley FOREIGN KEY (medley_id) REFERENCES medleys (id) ON DELETE CASCADE,
    CONSTRAINT medley_stations_station FOREIGN KEY (station_id) REFERENCES stations (id) ON DELETE CASCADE
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4;

INSERT INTO medley_stations (medley_id, station_id, position)
SELECT id, (SELECT MIN(id) FROM stations), 0 FROM medleys;

-- No row: the restriction applies to every station.
CREATE TABLE restriction_stations (
    restriction_id  BIGINT UNSIGNED NOT NULL,
    station_id      BIGINT UNSIGNED NOT NULL,
    PRIMARY KEY (restriction_id, station_id),
    KEY restriction_stations_station (station_id),
    CONSTRAINT restriction_stations_restriction FOREIGN KEY (restriction_id) REFERENCES restrictions (id) ON DELETE CASCADE,
    CONSTRAINT restriction_stations_station FOREIGN KEY (station_id) REFERENCES stations (id) ON DELETE CASCADE
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4;

INSERT INTO station_settings (station_id, name, value)
SELECT (SELECT MIN(id) FROM stations), 'medley_categories',
       JSON_OBJECT(
           'category_id', JSON_EXTRACT(value, '$.category_id'),
           'liner_category_id', JSON_EXTRACT(value, '$.liner_category_id'),
           'opening_liner_category_id', JSON_EXTRACT(value, '$.opening_liner_category_id'),
           'closing_liner_category_id', JSON_EXTRACT(value, '$.closing_liner_category_id'))
FROM settings WHERE name = 'medleys';

UPDATE settings
SET value = JSON_REMOVE(value, '$.category_id', '$.liner_category_id',
                        '$.opening_liner_category_id', '$.closing_liner_category_id')
WHERE name = 'medleys';
