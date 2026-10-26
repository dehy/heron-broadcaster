// SPDX-License-Identifier: AGPL-3.0-or-later

// Light and dark themes. `system` follows the browser's setting
// (prefers-color-scheme); the choice is saved per user by the API and
// remembered in this browser so the first paint already uses it.

import type { Theme } from "../api";

const KEY = "heron.theme";

export function storedTheme(): Theme {
  try {
    const value = window.localStorage.getItem(KEY);
    if (value === "light" || value === "dark") {
      return value;
    }
  } catch {
    // Storage blocked: follow the system.
  }
  return "system";
}

export function applyTheme(theme: Theme) {
  const root = document.documentElement;
  if (theme === "system") {
    delete root.dataset.theme;
  } else {
    root.dataset.theme = theme;
  }
  try {
    window.localStorage.setItem(KEY, theme);
  } catch {
    // Not remembered in this browser; the API still has it.
  }
}

// The page's width: a centred column, or the whole screen (wide screens,
// long tables). Saved per user by the API, remembered in this browser too.

const WIDTH_KEY = "heron.fullWidth";

export function storedFullWidth(): boolean {
  try {
    return window.localStorage.getItem(WIDTH_KEY) === "true";
  } catch {
    return false;
  }
}

export function applyFullWidth(fullWidth: boolean) {
  const root = document.documentElement;
  if (fullWidth) {
    root.dataset.width = "full";
  } else {
    delete root.dataset.width;
  }
  try {
    window.localStorage.setItem(WIDTH_KEY, String(fullWidth));
  } catch {
    // Not remembered in this browser; the API still has it.
  }
}
