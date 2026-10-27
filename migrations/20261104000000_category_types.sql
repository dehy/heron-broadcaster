-- SPDX-License-Identifier: AGPL-3.0-or-later
--
-- Category types: what a category holds (songs, jingles, ads, or a type of
-- the station's own). Rules can then apply to every category of a type,
-- such as never two jingles in a row. See docs/rotation.md#category-types.

CREATE TABLE category_types (
    id          BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    name        VARCHAR(64) NOT NULL,
    created_at  DATETIME(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    PRIMARY KEY (id),
    UNIQUE KEY category_types_name (name)
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_unicode_ci;

-- The default types; the station can rename them, add its own, and delete
-- those no category uses.
INSERT INTO category_types (id, name) VALUES (1, 'Song'), (2, 'Jingle'), (3, 'Ad');

-- NULL: no type. Deleting a type still in use is refused.
ALTER TABLE categories
    ADD COLUMN type_id BIGINT UNSIGNED NULL AFTER color,
    ADD CONSTRAINT categories_type FOREIGN KEY (type_id) REFERENCES category_types (id);

-- Categories the FX player offers are jingles, liners and the like: a guess
-- for the station to check.
UPDATE categories SET type_id = IF(fx_player, 2, 1);
