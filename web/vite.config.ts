// SPDX-License-Identifier: AGPL-3.0-or-later

import react from "@vitejs/plugin-react";
import { defineConfig } from "vitest/config";

// In development, /api (and its WebSocket) is proxied to a heron-api
// running locally, or to a station's heron-web with HERON_API_URL.
export default defineConfig({
  plugins: [react()],
  server: {
    proxy: {
      "/api": { target: process.env.HERON_API_URL ?? "http://localhost:8080", ws: true },
    },
  },
  test: {
    include: ["src/**/*.test.ts"],
  },
});
