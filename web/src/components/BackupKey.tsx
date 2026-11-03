// SPDX-License-Identifier: AGPL-3.0-or-later

// The backup key: typed for a check, a restore or a test (docs/backups.md#encryption).

import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { Link } from "../lib/router";
import { api, done, errorMessage, unwrap } from "../api";
import { rich, t } from "../i18n";
import { looksLikeKey } from "../lib/backups";
import { dateTime } from "../lib/format";
import { Banner, Field, Modal, useToast } from "./ui";

/** A public key in full, `age1…`: what tells one saved key from another. */
export function PublicKey({ recipient }: Readonly<{ recipient: string }>) {
  return (
    <code className="mono" style={{ overflowWrap: "anywhere" }}>
      {recipient}
    </code>
  );
}

/**
 * A field for the secret key: hidden as a password, never kept. With
 * `recipient`, the public key it must match, so that the right one is
 * picked from the password manager.
 */
export function KeyInput({
  value,
  onChange,
  autoFocus,
  recipient,
}: Readonly<{
  value: string;
  onChange: (value: string) => void;
  autoFocus?: boolean;
  recipient?: string | null | undefined;
}>) {
  return (
    <>
      <Field label={t("backupKey.key")} hint={t("backupKey.keyHint")}>
        <input
          className="input mono"
          type="password"
          autoComplete="off"
          spellCheck={false}
          autoFocus={autoFocus}
          placeholder="AGE-SECRET-KEY-1…"
          value={value}
          onChange={(event) => onChange(event.target.value)}
        />
      </Field>
      {recipient && (
        <p className="muted small" style={{ margin: 0 }}>
          {rich("backupKey.matches", { key: <PublicKey recipient={recipient} /> })}
        </p>
      )}
    </>
  );
}

/** Invalidated whenever the key or a backup changes. */
export const BACKUP_KEYS = [["backups"], ["backups", "key"], ["backups", "state"], ["backups", "tests"]];

/**
 * Asks an admin to type the backup key back: their next check further
 * away, and the newest backup tested with it. As a reminder, it can wait a
 * day, or say how to change a key the admin does not have.
 */
export function KeyCheckDialog({ reminder, onClose }: Readonly<{ reminder: boolean; onClose: () => void }>) {
  const client = useQueryClient();
  const toast = useToast();
  const [key, setKey] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [missing, setMissing] = useState(false);
  const current = useQuery({
    queryKey: ["backups", "key"],
    queryFn: () => unwrap(api.GET("/api/v1/backups/key")),
  });
  const refresh = async () => {
    for (const queryKey of BACKUP_KEYS) {
      await client.invalidateQueries({ queryKey });
    }
  };
  const check = async () => {
    setBusy(true);
    setError(null);
    try {
      const checked = await unwrap(api.POST("/api/v1/backups/key/check", { body: { key } }));
      toast(
        checked.testing
          ? t("backupKey.checkedTesting", { date: dateTime(checked.next_check_unix_ms) })
          : t("backupKey.checked", { date: dateTime(checked.next_check_unix_ms) }),
      );
      setKey("");
      await refresh();
      onClose();
    } catch (failure) {
      setError(errorMessage(failure));
    } finally {
      setBusy(false);
    }
  };
  const later = async () => {
    try {
      await done(api.POST("/api/v1/backups/key/later"));
    } catch (failure) {
      toast(errorMessage(failure), true);
    }
    await refresh();
    onClose();
  };
  const owner = current.data?.current;
  return (
    <Modal
      title={t("backupKey.checkTitle")}
      open
      onClose={reminder ? () => void later() : onClose}
      footer={
        <>
          {reminder ? (
            <>
              <button type="button" className="btn ghost" onClick={() => setMissing(true)}>
                {t("backupKey.dontHave")}
              </button>
              <button type="button" className="btn" onClick={() => void later()}>
                {t("backupKey.later")}
              </button>
            </>
          ) : (
            <button type="button" className="btn" onClick={onClose}>
              {t("ui.cancel")}
            </button>
          )}
          <button
            type="button"
            className="btn primary"
            disabled={busy || !looksLikeKey(key)}
            onClick={() => void check()}
          >
            {busy ? t("backupKey.checking") : t("backupKey.check")}
          </button>
        </>
      }
    >
      <form
        className="stack"
        onSubmit={(event) => {
          event.preventDefault();
          if (looksLikeKey(key)) {
            void check();
          }
        }}
      >
        <p style={{ margin: 0 }}>{reminder ? t("backupKey.reminder") : t("backupKey.checkHint")}</p>
        <KeyInput value={key} onChange={setKey} autoFocus recipient={owner?.recipient} />
        {error && <Banner kind="bad">{error}</Banner>}
        {missing && (
          <Banner kind="warn">
            {owner
              ? rich("backupKey.missing", {
                  user: <strong>{owner.set_by}</strong>,
                  date: dateTime(owner.since_unix_ms),
                  link: (
                    <Link to="/backups" onClick={onClose}>
                      {t("nav.backups")}
                    </Link>
                  ),
                })
              : t("backupKey.missingNoKey")}
          </Banner>
        )}
      </form>
    </Modal>
  );
}
