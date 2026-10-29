-- SPDX-License-Identifier: AGPL-3.0-or-later
--
-- Medleys made before the ducking placement go down over 0.3 s, like new
-- ones, still before the liner. Rendered files keep their saved layout.
-- See docs/medleys.md.

UPDATE medleys
SET recipe = JSON_SET(recipe, '$.duck_attack_ms', 300)
WHERE JSON_VALUE(recipe, '$.duck_attack_ms') = 150
  AND JSON_VALUE(recipe, '$.duck_attack_at') = 'outside';
