-- SPDX-License-Identifier: AGPL-3.0-or-later
--
-- Where the music goes down and comes back up around a liner over it
-- (heron_core::medley::DuckPlace). A medley made before keeps how it was
-- mixed: down over 0.15 s before the liner, back up during it. New medleys
-- take the settings' defaults. See docs/medleys.md.

UPDATE medleys
SET recipe = JSON_SET(recipe,
        '$.duck_attack_ms', 150,
        '$.duck_attack_at', 'outside',
        '$.duck_release_at', 'inside')
WHERE JSON_EXTRACT(recipe, '$.duck_attack_at') IS NULL;
