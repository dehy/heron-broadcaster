-- SPDX-License-Identifier: AGPL-3.0-or-later
--
-- Categories the FX player picks from: liners, power intros, voice tracks,
-- played over the title on air. The FX player's picker opens on them;
-- clocks still call them like any other category. See docs/web.md.

ALTER TABLE categories
    ADD COLUMN fx_player BOOLEAN NOT NULL DEFAULT FALSE AFTER color;
