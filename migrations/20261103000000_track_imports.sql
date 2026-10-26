-- SPDX-License-Identifier: AGPL-3.0-or-later
--
-- Importing track details from another radio automation software: tags,
-- categories, fades and markers, from a JSON Lines export. Every import
-- keeps what it changed, so it can be undone. See docs/import.md.

-- Points of a track Heron does not play by itself yet, kept for the
-- programmers: where the intro ends (the vocals start), a second intro, the
-- hook (the most recognizable part), and where the next track starts after
-- it (softly, with a fade, or hard). In ms on the file's own timeline.
CREATE TABLE track_markers (
    track_id      BIGINT UNSIGNED NOT NULL,
    intro_ms      INT UNSIGNED NULL,
    intro2_ms     INT UNSIGNED NULL,
    hook_in_ms    INT UNSIGNED NULL,
    hook_out_ms   INT UNSIGNED NULL,
    next_soft_ms  INT UNSIGNED NULL,
    next_hard_ms  INT UNSIGNED NULL,
    updated_at    DATETIME(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6) ON UPDATE CURRENT_TIMESTAMP(6),
    PRIMARY KEY (track_id),
    CONSTRAINT track_markers_track FOREIGN KEY (track_id) REFERENCES tracks (id) ON DELETE CASCADE
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4;

CREATE TABLE track_imports (
    id           BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    created_by   BIGINT UNSIGNED NULL,
    created_at   DATETIME(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    -- Tracks changed, categories and labels created.
    summary      JSON NOT NULL,
    undone_at    DATETIME(6) NULL,
    undone_by    BIGINT UNSIGNED NULL,
    PRIMARY KEY (id),
    CONSTRAINT track_imports_created_by FOREIGN KEY (created_by) REFERENCES users (id) ON DELETE SET NULL,
    CONSTRAINT track_imports_undone_by FOREIGN KEY (undone_by) REFERENCES users (id) ON DELETE SET NULL
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4;

-- What an import changed on a track, as it was before: undoing puts it back.
CREATE TABLE track_import_changes (
    import_id  BIGINT UNSIGNED NOT NULL,
    track_id   BIGINT UNSIGNED NOT NULL,
    previous   JSON NOT NULL,
    PRIMARY KEY (import_id, track_id),
    KEY track_import_changes_track (track_id),
    CONSTRAINT track_import_changes_import FOREIGN KEY (import_id) REFERENCES track_imports (id) ON DELETE CASCADE,
    CONSTRAINT track_import_changes_track FOREIGN KEY (track_id) REFERENCES tracks (id) ON DELETE CASCADE
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4;
