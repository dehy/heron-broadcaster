// SPDX-License-Identifier: AGPL-3.0-or-later

import { useQueryClient } from "@tanstack/react-query";
import { type FormEvent, useEffect, useState } from "react";
import { api, errorMessage, unwrap } from "../api";
import { Banner, Field, Logo } from "../components/ui";
import { ME_KEY } from "../lib/session";

export function Login() {
  const client = useQueryClient();
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    document.title = "Sign in · Heron Broadcaster";
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
      setError(errorMessage(failure));
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
            Keeping the air on, 24/7.
          </p>
          {error && <Banner kind="bad">{error}</Banner>}
          <Field label="Username">
            <input
              className="input"
              autoComplete="username"
              value={username}
              onChange={(event) => setUsername(event.target.value)}
              required
              autoFocus
            />
          </Field>
          <Field label="Password">
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
            {busy ? "Signing in…" : "Sign in"}
          </button>
          <p className="muted small" style={{ margin: 0 }}>
            The first admin is created on the server:{" "}
            <code>heron-api user add admin --role admin</code>
          </p>
        </div>
      </form>
    </main>
  );
}
