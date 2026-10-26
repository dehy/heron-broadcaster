-- SPDX-License-Identifier: AGPL-3.0-or-later
--
-- Whether the web UI uses the whole width of the screen for each user,
-- instead of a centred column (for wide screens and long tables).

ALTER TABLE users
    ADD COLUMN full_width BOOLEAN NOT NULL DEFAULT FALSE AFTER language;
