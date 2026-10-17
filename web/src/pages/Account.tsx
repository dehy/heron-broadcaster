// SPDX-License-Identifier: AGPL-3.0-or-later

// Your account: theme, password and API tokens.

import { useQuery, useQueryClient } from "@tanstack/react-query";
import { type FormEvent, useState } from "react";
import { type Theme, api, done, unwrap } from "../api";
import { Banner, Card, ConfirmButton, Empty, Field, Icon, NumberInput, PageHead, Segmented } from "../components/ui";
import { ago, dateTime } from "../lib/format";
import { ME_KEY, useAction, useUser } from "../lib/session";
import { applyTheme } from "../lib/theme";

export function Account() {
  const user = useUser();
  return (
    <>
      <PageHead title="Account" description={`Signed in as ${user.username}, ${user.role}.`} />
      <div className="grid two">
        <ThemeCard theme={user.theme} />
        <PasswordCard />
      </div>
      <TokensCard />
    </>
  );
}

function ThemeCard({ theme }: { theme: Theme }) {
  const client = useQueryClient();
  const save = useAction(
    (next: Theme) => unwrap(api.PUT("/api/v1/auth/preferences", { body: { theme: next } })),
    { onSuccess: (user) => client.setQueryData(ME_KEY, user) },
  );
  return (
    <Card title="Theme">
      <div className="stack">
        <Segmented
          label="Theme"
          value={theme}
          onChange={(next) => {
            applyTheme(next);
            save.mutate(next);
          }}
          options={[
            { value: "system", label: <><Icon name="monitor" size={14} /> System</> },
            { value: "light", label: <><Icon name="sun" size={14} /> Light</> },
            { value: "dark", label: <><Icon name="moon" size={14} /> Dark</> },
          ]}
        />
        <p className="muted small" style={{ margin: 0 }}>
          System follows your device's setting. Your choice follows you on every browser you sign in
          with.
        </p>
      </div>
    </Card>
  );
}

function PasswordCard() {
  const [current, setCurrent] = useState("");
  const [next, setNext] = useState("");
  const change = useAction(
    () =>
      done(api.PUT("/api/v1/auth/password", { body: { current_password: current, new_password: next } })),
    {
      success: "Password changed: your other sessions are signed out",
      onSuccess: () => {
        setCurrent("");
        setNext("");
      },
    },
  );
  const submit = (event: FormEvent) => {
    event.preventDefault();
    change.mutate(undefined);
  };
  return (
    <Card title="Password">
      <form className="stack" onSubmit={submit}>
        <Field label="Current password">
          <input
            className="input"
            type="password"
            autoComplete="current-password"
            value={current}
            onChange={(e) => setCurrent(e.target.value)}
            required
          />
        </Field>
        <Field label="New password" hint="8 characters at least">
          <input
            className="input"
            type="password"
            autoComplete="new-password"
            minLength={8}
            value={next}
            onChange={(e) => setNext(e.target.value)}
            required
          />
        </Field>
        <div className="row end">
          <button type="submit" className="btn primary" disabled={change.isPending}>
            Change
          </button>
        </div>
      </form>
    </Card>
  );
}

function TokensCard() {
  const tokens = useQuery({ queryKey: ["tokens"], queryFn: () => unwrap(api.GET("/api/v1/tokens")) });
  const [name, setName] = useState("");
  const [days, setDays] = useState(0);
  const [created, setCreated] = useState<string | null>(null);
  const create = useAction(
    () =>
      unwrap(
        api.POST("/api/v1/tokens", {
          body: days > 0 ? { name, expires_in_days: days } : { name },
        }),
      ),
    {
      invalidate: [["tokens"]],
      onSuccess: (token) => {
        setCreated(token.token);
        setName("");
      },
    },
  );
  const revoke = useAction(
    (id: number) => done(api.DELETE("/api/v1/tokens/{id}", { params: { path: { id } } })),
    { success: "Token revoked", invalidate: [["tokens"]] },
  );
  return (
    <Card title="API tokens">
      <div className="stack">
        <p className="muted small" style={{ margin: 0 }}>
          For scripts: send <code>Authorization: Bearer heron_…</code>. A token acts as you, with your
          role.
        </p>
        {created && (
          <Banner kind="warn">
            Copy this token now, it is not shown again: <code style={{ overflowWrap: "anywhere" }}>{created}</code>
          </Banner>
        )}
        <form
          className="row"
          onSubmit={(event) => {
            event.preventDefault();
            create.mutate(undefined);
          }}
        >
          <label style={{ flex: "1 1 12rem" }}>
            <span className="visually-hidden">Token name</span>
            <input
              className="input"
              placeholder="What it is for"
              value={name}
              onChange={(e) => setName(e.target.value)}
              required
            />
          </label>
          <label className="row small muted" style={{ flexWrap: "nowrap" }}>
            Expires after
            <span style={{ width: "5rem" }}>
              <NumberInput value={days} min={0} max={3650} step={1} onChange={(d) => setDays(Math.round(d))} />
            </span>
            days (0: never)
          </label>
          <button type="submit" className="btn primary" disabled={create.isPending}>
            Create
          </button>
        </form>
        {tokens.data?.length === 0 && <Empty>No token.</Empty>}
        <ul className="list">
          {tokens.data?.map((token) => (
            <li key={token.id} className="row between" style={{ padding: "0.5rem 0" }}>
              <span>
                <strong>{token.name}</strong>
                <span className="muted small">
                  {" "}
                  · created {dateTime(token.created_at_unix_ms)} · used {ago(token.last_used_at_unix_ms)}
                  {token.expires_at_unix_ms && ` · expires ${dateTime(token.expires_at_unix_ms)}`}
                </span>
              </span>
              <ConfirmButton
                className="btn small danger"
                confirm="Revoke"
                question={<p>Revoke {token.name}? Scripts using it stop working.</p>}
                onConfirm={() => revoke.mutate(token.id)}
              >
                Revoke
              </ConfirmButton>
            </li>
          ))}
        </ul>
      </div>
    </Card>
  );
}
