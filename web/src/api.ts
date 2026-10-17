// SPDX-License-Identifier: AGPL-3.0-or-later

// Hand-written until M4, when client types are generated from the OpenAPI spec.
export interface ApiStatus {
  service: string;
  version: string;
  database: "ok" | "unavailable";
}

export interface LibraryStatus {
  tracks: number;
  missing: number;
  analyzed: number;
  silent: number;
  analysis_queue: { pending: number; running: number; failed: number };
  last_scan: { finished_at_unix: number; duration_ms: number; files: number } | null;
}

async function getJson<T>(path: string, signal?: AbortSignal): Promise<T> {
  const response = await fetch(path, signal ? { signal } : {});
  if (!response.ok) {
    throw new Error(`API answered ${response.status}`);
  }
  return (await response.json()) as T;
}

export const fetchStatus = (signal?: AbortSignal) => getJson<ApiStatus>("/api/v1/status", signal);

export const fetchLibraryStatus = (signal?: AbortSignal) =>
  getJson<LibraryStatus>("/api/v1/library/status", signal);
