// SPDX-License-Identifier: AGPL-3.0-or-later

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { ApiError } from "./api";
import { App } from "./App";
import { ToastProvider } from "./components/ui";
import { startLanguage } from "./i18n";
import "./index.css";
import { startErrorReporting } from "./sentry";

startErrorReporting();

const queries = new QueryClient({
  defaultOptions: {
    queries: {
      staleTime: 5_000,
      refetchOnWindowFocus: true,
      // Client errors (not found, forbidden) do not get better by retrying.
      retry: (failures, error) =>
        failures < 2 && !(error instanceof ApiError && error.status < 500),
    },
  },
});

const root = document.getElementById("root");
if (!root) {
  throw new Error("missing #root element");
}

// The language this browser remembers is loaded before the first paint.
void startLanguage().then(() =>
  createRoot(root).render(
    <StrictMode>
      <QueryClientProvider client={queries}>
        <ToastProvider>
          <App />
        </ToastProvider>
      </QueryClientProvider>
    </StrictMode>,
  ),
);
