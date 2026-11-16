-- SPDX-License-Identifier: AGPL-3.0-or-later
--
-- Opus files are decoded now (docs/analysis.md#analyzer). Their analyses
-- failed for good before ("unsupported audio codec"), and failed jobs are
-- not retried while the file is unchanged: forgetting them lets the next
-- scan queue those tracks again, and only those.

DELETE j FROM analysis_jobs j
JOIN tracks t ON t.id = j.track_id
WHERE j.status = 'failed'
  AND (t.codec = 'Opus' OR LOWER(t.path) LIKE '%.opus');
