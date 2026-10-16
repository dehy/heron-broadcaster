// SPDX-License-Identifier: AGPL-3.0-or-later

import { useEffect, useState } from "react";
import { type ApiStatus, fetchStatus } from "./api";

type State =
  | { kind: "loading" }
  | { kind: "ok"; status: ApiStatus }
  | { kind: "error"; message: string };

const REFRESH_MS = 5000;

export function App() {
  const [state, setState] = useState<State>({ kind: "loading" });

  useEffect(() => {
    const controller = new AbortController();
    const load = () => {
      fetchStatus(controller.signal)
        .then((status) => setState({ kind: "ok", status }))
        .catch((error: unknown) => {
          if (!controller.signal.aborted) {
            setState({ kind: "error", message: String(error) });
          }
        });
    };
    load();
    const timer = setInterval(load, REFRESH_MS);
    return () => {
      controller.abort();
      clearInterval(timer);
    };
  }, []);

  return (
    <main>
      <h1>Heron Broadcaster</h1>
      <p className="tagline">Keeping the air on, 24/7.</p>
      <section aria-live="polite">
        <h2>System status</h2>
        <StatusView state={state} />
      </section>
    </main>
  );
}

function StatusView({ state }: { state: State }) {
  switch (state.kind) {
    case "loading":
      return <p>Contacting the API…</p>;
    case "error":
      return <p className="bad">API unreachable: {state.message}</p>;
    case "ok":
      return (
        <dl>
          <dt>API</dt>
          <dd className="good">
            {state.status.service} {state.status.version}
          </dd>
          <dt>Database</dt>
          <dd className={state.status.database === "ok" ? "good" : "bad"}>
            {state.status.database}
          </dd>
        </dl>
      );
  }
}
