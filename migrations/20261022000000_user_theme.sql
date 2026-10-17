-- SPDX-License-Identifier: AGPL-3.0-or-later
--
-- M5: the theme each user picked in the web UI. `system` follows the
-- browser's setting.

ALTER TABLE users
    ADD COLUMN theme ENUM('system', 'light', 'dark') NOT NULL DEFAULT 'system' AFTER enabled;
