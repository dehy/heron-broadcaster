-- SPDX-License-Identifier: AGPL-3.0-or-later
--
-- M4: API and control plane. See docs/api.md.

-- How each play ended, from the engine's track-change events. NULL while
-- the track plays, and when the API missed the end (it was down).
ALTER TABLE play_history
    ADD COLUMN played_ms INT UNSIGNED NULL AFTER ended_at,
    ADD COLUMN end_reason ENUM('finished', 'skipped', 'stopped', 'replaced') NULL AFTER played_ms,
    ADD KEY play_history_open (ended_at);
