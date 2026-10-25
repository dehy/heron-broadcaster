-- SPDX-License-Identifier: AGPL-3.0-or-later
--
-- Webhooks on track changes, and their deliveries: the delivery log, and
-- the retry queue (a pending delivery is sent again at next_attempt_at).
-- See docs/api.md#webhooks.
--
-- Replaces the placeholder tables of the initial schema, which nothing
-- ever used.

DROP TABLE webhook_deliveries;
DROP TABLE webhooks;

CREATE TABLE webhooks (
    id             BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    name           VARCHAR(255) NOT NULL,
    url            VARCHAR(2048) NOT NULL,
    enabled        BOOLEAN NOT NULL DEFAULT TRUE,
    -- Subscribed events: track.started, track.ended, track.skipped.
    events         JSON NOT NULL,
    -- Also send the FX player's tracks (jingles, liners).
    include_fx     BOOLEAN NOT NULL DEFAULT FALSE,
    -- Signs the deliveries (HMAC-SHA256); only shown when it is created.
    secret         VARCHAR(255) NOT NULL,
    content_type   VARCHAR(255) NOT NULL DEFAULT 'application/json',
    -- With {{placeholders}}; NULL sends the default JSON body.
    body_template  TEXT NULL,
    created_at     DATETIME(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    updated_at     DATETIME(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6) ON UPDATE CURRENT_TIMESTAMP(6),
    PRIMARY KEY (id),
    UNIQUE KEY webhooks_name (name)
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4;

CREATE TABLE webhook_deliveries (
    id                BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    webhook_id        BIGINT UNSIGNED NOT NULL,
    -- track.started, track.ended, track.skipped or ping.
    event             VARCHAR(32) NOT NULL,
    body              MEDIUMTEXT NOT NULL,
    status            ENUM('pending', 'delivered', 'failed') NOT NULL DEFAULT 'pending',
    attempts          SMALLINT UNSIGNED NOT NULL DEFAULT 0,
    -- While pending.
    next_attempt_at   DATETIME(6) NULL,
    last_attempt_at   DATETIME(6) NULL,
    last_status_code  SMALLINT UNSIGNED NULL,
    last_error        VARCHAR(500) NULL,
    -- The start of the receiver's last answer.
    last_response     VARCHAR(1024) NULL,
    created_at        DATETIME(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    delivered_at      DATETIME(6) NULL,
    PRIMARY KEY (id),
    KEY webhook_deliveries_due (status, next_attempt_at),
    KEY webhook_deliveries_log (webhook_id, created_at),
    KEY webhook_deliveries_created (created_at),
    CONSTRAINT webhook_deliveries_webhook FOREIGN KEY (webhook_id) REFERENCES webhooks (id) ON DELETE CASCADE
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4;
