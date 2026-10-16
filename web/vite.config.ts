// SPDX-License-Identifier: AGPL-3.0-or-later

import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

// In development, /api is proxied to a heron-api running locally.
export default defineConfig({
  plugins: [react()],
  server: {
    proxy: {
      "/api": process.env.HERON_API_URL ?? "http://localhost:8080",
    },
  },
});
