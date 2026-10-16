// SPDX-License-Identifier: AGPL-3.0-or-later

// Hand-written until M4, when client types are generated from the OpenAPI spec.
export interface ApiStatus {
  service: string;
  version: string;
  database: "ok" | "unavailable";
}

export async function fetchStatus(signal?: AbortSignal): Promise<ApiStatus> {
  const response = await fetch("/api/v1/status", signal ? { signal } : {});
  if (!response.ok) {
    throw new Error(`API answered ${response.status}`);
  }
  return (await response.json()) as ApiStatus;
}
