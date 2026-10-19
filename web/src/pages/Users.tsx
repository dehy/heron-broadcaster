// SPDX-License-Identifier: AGPL-3.0-or-later

// Users and their roles (admins).

import { useQuery } from "@tanstack/react-query";
import { useState } from "react";
import { type Role, type User, api, done, unwrap } from "../api";
import { Banner, Card, ConfirmButton, ErrorView, Field, Icon, Loading, Modal, PageHead } from "../components/ui";
import { type PlainKey, t } from "../i18n";
import { ago } from "../lib/format";
import { useAction, useCan, useUser } from "../lib/session";

const ROLES: { value: Role; label: PlainKey; hint: PlainKey }[] = [
  { value: "viewer", label: "users.role.viewer", hint: "users.role.viewerHint" },
  { value: "operator", label: "users.role.operator", hint: "users.role.operatorHint" },
  { value: "admin", label: "users.role.admin", hint: "users.role.adminHint" },
];

export function Users() {
  const isAdmin = useCan("admin");
  const me = useUser();
  const users = useQuery({
    queryKey: ["users"],
    queryFn: () => unwrap(api.GET("/api/v1/users")),
    enabled: isAdmin,
  });
  const [creating, setCreating] = useState(false);
  const [resetting, setResetting] = useState<User | null>(null);
  const update = useAction(
    ({ id, ...body }: { id: number; role?: Role; enabled?: boolean }) =>
      unwrap(api.PATCH("/api/v1/users/{id}", { params: { path: { id } }, body })),
    { success: t("categories.saved"), invalidate: [["users"]] },
  );
  const remove = useAction(
    (id: number) => done(api.DELETE("/api/v1/users/{id}", { params: { path: { id } } })),
    { success: t("users.deleted"), invalidate: [["users"]] },
  );

  if (!isAdmin) {
    return <Banner kind="warn">{t("users.adminsOnly")}</Banner>;
  }
  return (
    <>
      <PageHead
        title={t("nav.users")}
        actions={
          <button type="button" className="btn primary" onClick={() => setCreating(true)}>
            <Icon name="plus" /> {t("users.new")}
          </button>
        }
      />
      <Card flush>
        {users.isPending && <Loading />}
        {users.isError && <ErrorView error={users.error} />}
        {users.data && (
          <div className="table-wrap">
            <table className="table">
              <thead>
                <tr>
                  <th>{t("users.user")}</th>
                  <th>{t("users.role")}</th>
                  <th className="hide-narrow">{t("users.lastSignIn")}</th>
                  <th>
                    <span className="visually-hidden">{t("library.actions")}</span>
                  </th>
                </tr>
              </thead>
              <tbody>
                {users.data.map((user) => (
                  <tr key={user.id}>
                    <td>
                      {user.username} {user.id === me.id && <span className="badge accent">{t("users.you")}</span>}{" "}
                      {!user.enabled && <span className="badge">{t("output.disabled")}</span>}
                    </td>
                    <td>
                      <select
                        className="input"
                        value={user.role}
                        aria-label={t("users.roleOf", { name: user.username })}
                        onChange={(e) => update.mutate({ id: user.id, role: e.target.value as Role })}
                      >
                        {ROLES.map((role) => (
                          <option key={role.value} value={role.value}>
                            {t(role.label)}
                          </option>
                        ))}
                      </select>
                    </td>
                    <td className="hide-narrow muted">{ago(user.last_login_at_unix_ms)}</td>
                    <td className="actions">
                      <button type="button" className="btn small" onClick={() => setResetting(user)}>
                        {t("login.password")}
                      </button>
                      {user.id !== me.id && (
                        <>
                          <button
                            type="button"
                            className="btn small"
                            onClick={() => update.mutate({ id: user.id, enabled: !user.enabled })}
                          >
                            {user.enabled ? t("track.disable") : t("track.enable")}
                          </button>
                          <ConfirmButton
                            className="btn ghost small icon danger"
                            title={t("categories.delete", { name: user.username })}
                            confirm={t("categories.deleteButton")}
                            question={<p>{t("users.deleteQuestion", { name: user.username })}</p>}
                            onConfirm={() => remove.mutate(user.id)}
                          >
                            <Icon name="trash" />
                          </ConfirmButton>
                        </>
                      )}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </Card>
      <dl className="facts small">
        {ROLES.map((role) => (
          <div key={role.value} style={{ display: "contents" }}>
            <dt>{t(role.label)}</dt>
            <dd>{t(role.hint)}</dd>
          </div>
        ))}
      </dl>
      {creating && <NewUserDialog onClose={() => setCreating(false)} />}
      {resetting && <PasswordDialog user={resetting} onClose={() => setResetting(null)} />}
    </>
  );
}

function NewUserDialog({ onClose }: { onClose: () => void }) {
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [role, setRole] = useState<Role>("operator");
  const create = useAction(
    () => unwrap(api.POST("/api/v1/users", { body: { username, password, role } })),
    { success: t("users.created"), invalidate: [["users"]], onSuccess: onClose },
  );
  return (
    <Modal
      title={t("users.new")}
      open
      onClose={onClose}
      footer={
        <>
          <button type="button" className="btn" onClick={onClose}>
            {t("common.cancel")}
          </button>
          <button type="submit" form="user-form" className="btn primary" disabled={create.isPending}>
            {t("account.create")}
          </button>
        </>
      }
    >
      <form
        id="user-form"
        className="stack"
        onSubmit={(event) => {
          event.preventDefault();
          create.mutate(undefined);
        }}
      >
        <Field label={t("login.username")}>
          <input className="input" value={username} onChange={(e) => setUsername(e.target.value)} required autoComplete="off" />
        </Field>
        <Field label={t("login.password")} hint={t("account.passwordHint")}>
          <input
            className="input"
            type="password"
            value={password}
            minLength={8}
            onChange={(e) => setPassword(e.target.value)}
            required
            autoComplete="new-password"
          />
        </Field>
        <Field label={t("users.role")} hint={t(ROLES.find((r) => r.value === role)?.hint ?? "users.role.operatorHint")}>
          <select className="input" value={role} onChange={(e) => setRole(e.target.value as Role)}>
            {ROLES.map((r) => (
              <option key={r.value} value={r.value}>
                {t(r.label)}
              </option>
            ))}
          </select>
        </Field>
      </form>
    </Modal>
  );
}

function PasswordDialog({ user, onClose }: { user: User; onClose: () => void }) {
  const [password, setPassword] = useState("");
  const reset = useAction(
    () => unwrap(api.PATCH("/api/v1/users/{id}", { params: { path: { id: user.id } }, body: { password } })),
    { success: t("users.passwordSet", { name: user.username }), onSuccess: onClose },
  );
  return (
    <Modal
      title={t("users.passwordOf", { name: user.username })}
      open
      onClose={onClose}
      footer={
        <>
          <button type="button" className="btn" onClick={onClose}>
            {t("common.cancel")}
          </button>
          <button type="submit" form="password-form" className="btn primary" disabled={reset.isPending}>
            {t("users.set")}
          </button>
        </>
      }
    >
      <form
        id="password-form"
        className="stack"
        onSubmit={(event) => {
          event.preventDefault();
          reset.mutate(undefined);
        }}
      >
        <p className="muted small" style={{ margin: 0 }}>
          {t("users.signedOut")}
        </p>
        <Field label={t("account.newPassword")} hint={t("account.passwordHint")}>
          <input
            className="input"
            type="password"
            value={password}
            minLength={8}
            onChange={(e) => setPassword(e.target.value)}
            required
            autoComplete="new-password"
          />
        </Field>
      </form>
    </Modal>
  );
}
