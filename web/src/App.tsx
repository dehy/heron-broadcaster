// SPDX-License-Identifier: AGPL-3.0-or-later

import { useEffect, useState } from "react";
import { type ApiStatus, type LibraryStatus, fetchLibraryStatus, fetchStatus } from "./api";

type Loaded<T> = { kind: "loading" } | { kind: "ok"; value: T } | { kind: "error"; message: string };

const REFRESH_MS = 5000;

/** Polls `load` every few seconds. */
function usePolling<T>(load: (signal: AbortSignal) => Promise<T>): Loaded<T> {
  const [state, setState] = useState<Loaded<T>>({ kind: "loading" });
  useEffect(() => {
    const controller = new AbortController();
    const refresh = () => {
      load(controller.signal)
        .then((value) => setState({ kind: "ok", value }))
        .catch((error: unknown) => {
          if (!controller.signal.aborted) {
            setState({ kind: "error", message: String(error) });
          }
        });
    };
    refresh();
    const timer = setInterval(refresh, REFRESH_MS);
    return () => {
      controller.abort();
      clearInterval(timer);
    };
  }, [load]);
  return state;
}

export function App() {
  const status = usePolling(fetchStatus);
  const library = usePolling(fetchLibraryStatus);

  return (
    <main>
      <h1>Heron Broadcaster</h1>
      <p className="tagline">Keeping the air on, 24/7.</p>
      <section aria-live="polite">
        <h2>System status</h2>
        <StatusView state={status} />
      </section>
      <section aria-live="polite">
        <h2>Library</h2>
        <LibraryView state={library} />
      </section>
    </main>
  );
}

function StatusView({ state }: { state: Loaded<ApiStatus> }) {
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
            {state.value.service} {state.value.version}
          </dd>
          <dt>Database</dt>
          <dd className={state.value.database === "ok" ? "good" : "bad"}>{state.value.database}</dd>
        </dl>
      );
  }
}

function LibraryView({ state }: { state: Loaded<LibraryStatus> }) {
  switch (state.kind) {
    case "loading":
      return <p>Loading…</p>;
    case "error":
      return <p className="bad">Library status unavailable: {state.message}</p>;
    case "ok": {
      const library = state.value;
      const queue = library.analysis_queue;
      const percent = library.tracks > 0 ? Math.floor((library.analyzed / library.tracks) * 100) : 100;
      return (
        <>
          <dl>
            <dt>Tracks</dt>
            <dd>{library.tracks.toLocaleString()}</dd>
            <dt>Analyzed</dt>
            <dd>
              {library.analyzed.toLocaleString()} ({percent} %)
            </dd>
            {queue.pending + queue.running > 0 && (
              <>
                <dt>In progress</dt>
                <dd>{(queue.pending + queue.running).toLocaleString()} queued</dd>
              </>
            )}
            {queue.failed > 0 && (
              <>
                <dt>Failed</dt>
                <dd className="bad">{queue.failed.toLocaleString()} unreadable files</dd>
              </>
            )}
            {library.silent > 0 && (
              <>
                <dt>Silent</dt>
                <dd className="bad">{library.silent.toLocaleString()} tracks with no audio</dd>
              </>
            )}
            {library.missing > 0 && (
              <>
                <dt>Missing</dt>
                <dd className="bad">{library.missing.toLocaleString()} files no longer found</dd>
              </>
            )}
          </dl>
          <progress max={100} value={percent} aria-label="Analysis progress" />
          {library.last_scan && (
            <p className="muted">
              Last scan {new Date(library.last_scan.finished_at_unix * 1000).toLocaleString()},{" "}
              {library.last_scan.files.toLocaleString()} files in{" "}
              {(library.last_scan.duration_ms / 1000).toFixed(1)} s
            </p>
          )}
        </>
      );
    }
  }
}
