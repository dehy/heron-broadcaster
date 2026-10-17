// SPDX-License-Identifier: AGPL-3.0-or-later

// Users and their roles (admins).

import { useQuery } from "@tanstack/react-query";
import { useState } from "react";
import { type Role, type User, api, done, unwrap } from "../api";
import { Banner, Card, ConfirmButton, ErrorView, Field, Icon, Loading, Modal, PageHead } from "../components/ui";
import { ago } from "../lib/format";
import { useAction, useCan, useUser } from "../lib/session";

const ROLES: { value: Role; label: string; hint: string }[] = [
  { value: "viewer", label: "Viewer", hint: "Sees everything except users and secrets" },
  { value: "operator", label: "Operator", hint: "Also runs the station: queue, skip, FX, tracks, categories" },
  { value: "admin", label: "Admin", hint: "Also settings, outputs and users" },
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
    { success: "Saved", invalidate: [["users"]] },
  );
  const remove = useAction(
    (id: number) => done(api.DELETE("/api/v1/users/{id}", { params: { path: { id } } })),
    { success: "User deleted", invalidate: [["users"]] },
  );

  if (!isAdmin) {
    return <Banner kind="warn">Only admins manage users.</Banner>;
  }
  return (
    <>
      <PageHead
        title="Users"
        actions={
          <button type="button" className="btn primary" onClick={() => setCreating(true)}>
            <Icon name="plus" /> New user
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
                  <th>User</th>
                  <th>Role</th>
                  <th className="hide-narrow">Last sign-in</th>
                  <th>
                    <span className="visually-hidden">Actions</span>
                  </th>
                </tr>
              </thead>
              <tbody>
                {users.data.map((user) => (
                  <tr key={user.id}>
                    <td>
                      {user.username} {user.id === me.id && <span className="badge accent">You</span>}{" "}
                      {!user.enabled && <span className="badge">Disabled</span>}
                    </td>
                    <td>
                      <select
                        className="input"
                        value={user.role}
                        aria-label={`Role of ${user.username}`}
                        onChange={(e) => update.mutate({ id: user.id, role: e.target.value as Role })}
                      >
                        {ROLES.map((role) => (
                          <option key={role.value} value={role.value}>
                            {role.label}
                          </option>
                        ))}
                      </select>
                    </td>
                    <td className="hide-narrow muted">{ago(user.last_login_at_unix_ms)}</td>
                    <td className="actions">
                      <button type="button" className="btn small" onClick={() => setResetting(user)}>
                        Password
                      </button>
                      {user.id !== me.id && (
                        <>
                          <button
                            type="button"
                            className="btn small"
                            onClick={() => update.mutate({ id: user.id, enabled: !user.enabled })}
                          >
                            {user.enabled ? "Disable" : "Enable"}
                          </button>
                          <ConfirmButton
                            className="btn ghost small icon danger"
                            title={`Delete ${user.username}`}
                            confirm="Delete"
                            question={<p>Delete {user.username}? Their sessions and API tokens stop working.</p>}
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
            <dt>{role.label}</dt>
            <dd>{role.hint}</dd>
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
    { success: "User created", invalidate: [["users"]], onSuccess: onClose },
  );
  return (
    <Modal
      title="New user"
      open
      onClose={onClose}
      footer={
        <>
          <button type="button" className="btn" onClick={onClose}>
            Cancel
          </button>
          <button type="submit" form="user-form" className="btn primary" disabled={create.isPending}>
            Create
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
        <Field label="Username">
          <input className="input" value={username} onChange={(e) => setUsername(e.target.value)} required autoComplete="off" />
        </Field>
        <Field label="Password" hint="8 characters at least">
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
        <Field label="Role" hint={ROLES.find((r) => r.value === role)?.hint}>
          <select className="input" value={role} onChange={(e) => setRole(e.target.value as Role)}>
            {ROLES.map((r) => (
              <option key={r.value} value={r.value}>
                {r.label}
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
    { success: `New password set for ${user.username}`, onSuccess: onClose },
  );
  return (
    <Modal
      title={`Password of ${user.username}`}
      open
      onClose={onClose}
      footer={
        <>
          <button type="button" className="btn" onClick={onClose}>
            Cancel
          </button>
          <button type="submit" form="password-form" className="btn primary" disabled={reset.isPending}>
            Set
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
          They are signed out everywhere.
        </p>
        <Field label="New password" hint="8 characters at least">
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
