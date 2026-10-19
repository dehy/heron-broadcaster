-- SPDX-License-Identifier: AGPL-3.0-or-later
--
-- M5d: the language each user picked in the web UI, a language tag (`en`,
-- `fr`…). `system` follows the browser's languages.

ALTER TABLE users
    ADD COLUMN language VARCHAR(16) NOT NULL DEFAULT 'system' AFTER theme;
