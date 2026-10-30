-- SPDX-License-Identifier: AGPL-3.0-or-later
--
-- A station's engine address is in heron-api's environment, not in the
-- database (docs/stations.md#engines): HERON_ENGINE_URL for the first
-- station, HERON_ENGINE_URL_<CODE> for the others. Codes lose '-', which a
-- variable name cannot hold.

ALTER TABLE stations DROP COLUMN engine_url;

UPDATE stations SET code = REPLACE(code, '-', '') WHERE code LIKE '%-%';
