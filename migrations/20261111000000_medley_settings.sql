-- SPDX-License-Identifier: AGPL-3.0-or-later
--
-- Medley settings: where rendered medleys go and where their liners come
-- from are set once for every medley (the `medleys` settings), and medleys
-- are named from their hooks when rendered. See docs/medleys.md.

-- The settings start with the category, and the liner category, most used
-- by the existing medleys. Tracks already rendered keep their categories.
INSERT INTO settings (name, value)
SELECT 'medleys', JSON_OBJECT(
    'category_id', (
        SELECT category_id FROM medleys WHERE category_id IS NOT NULL
        GROUP BY category_id ORDER BY COUNT(*) DESC, category_id LIMIT 1),
    'liner_category_id', (
        SELECT CAST(JSON_VALUE(m.recipe, '$.liner_category_id') AS UNSIGNED) AS liners
        FROM medleys m
        JOIN categories c ON c.id = CAST(JSON_VALUE(m.recipe, '$.liner_category_id') AS UNSIGNED)
        GROUP BY liners ORDER BY COUNT(*) DESC, liners LIMIT 1))
FROM DUAL
WHERE EXISTS (SELECT 1 FROM medleys)
  AND NOT EXISTS (SELECT 1 FROM settings WHERE name = 'medleys');

-- A recipe without a liner category had no liners: its liners are off.
UPDATE medleys
SET recipe = JSON_REMOVE(
    JSON_SET(recipe, '$.liners',
        IF(JSON_VALUE(recipe, '$.liner_category_id') IS NULL,
           JSON_EXTRACT('[false]', '$[0]'), JSON_EXTRACT('[true]', '$[0]'))),
    '$.liner_category_id');

-- A medley is named when it is rendered: none before.
ALTER TABLE medleys
    DROP FOREIGN KEY medleys_category,
    DROP COLUMN category_id,
    MODIFY COLUMN name VARCHAR(255) NULL;
