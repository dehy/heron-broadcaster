// SPDX-License-Identifier: AGPL-3.0-or-later

import { useQueryClient } from "@tanstack/react-query";
import { type FormEvent, useEffect, useState } from "react";
import { ApiError, api, errorMessage, unwrap } from "../api";
import { Banner, Field, Logo } from "../components/ui";
import { t } from "../i18n";
import { ME_KEY } from "../lib/session";

export function Login() {
  const client = useQueryClient();
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    document.title = t("app.title", { page: t("login.title") });
  }, []);

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    setBusy(true);
    setError(null);
    try {
      const user = await unwrap(
        api.POST("/api/v1/auth/login", { body: { username: username.trim(), password } }),
      );
      client.setQueryData(ME_KEY, user);
    } catch (failure) {
      // A wrong username or password answers 401, like a missing session.
      setError(
        failure instanceof ApiError && failure.code === "unauthorized"
          ? t("login.wrong")
          : errorMessage(failure),
      );
      setBusy(false);
    }
  };

  return (
    <main className="login">
      <form className="card" onSubmit={submit}>
        <div className="card-body stack">
          <div className="brand" style={{ padding: 0, fontSize: "1.3rem" }}>
            <Logo size={30} /> Heron Broadcaster
          </div>
          <p className="muted" style={{ margin: 0 }}>
            {t("login.tagline")}
          </p>
          {error && <Banner kind="bad">{error}</Banner>}
          <Field label={t("login.username")}>
            <input
              className="input"
              autoComplete="username"
              value={username}
              onChange={(event) => setUsername(event.target.value)}
              required
              autoFocus
            />
          </Field>
          <Field label={t("login.password")}>
            <input
              className="input"
              type="password"
              autoComplete="current-password"
              value={password}
              onChange={(event) => setPassword(event.target.value)}
              required
            />
          </Field>
          <button type="submit" className="btn primary big" disabled={busy}>
            {busy ? t("login.signingIn") : t("login.title")}
          </button>
          <p className="muted small" style={{ margin: 0 }}>
            {t("login.firstAdmin")}{" "}
            <code>heron-api user add admin --role admin</code>
          </p>
        </div>
      </form>
    </main>
  );
}
