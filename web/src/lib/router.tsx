// SPDX-License-Identifier: AGPL-3.0-or-later

// A small router on the History API: the app has a handful of pages, and
// nginx answers every path with index.html.

import { type AnchorHTMLAttributes, type MouseEvent, useSyncExternalStore } from "react";

const listeners = new Set<() => void>();

function notify() {
  for (const listener of listeners) {
    listener();
  }
}

window.addEventListener("popstate", notify);

function subscribe(listener: () => void) {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

const current = () => window.location.pathname + window.location.search;

/** Goes to `to` (a path with an optional query string). */
export function navigate(to: string, options: { replace?: boolean } = {}) {
  if (to === current()) {
    return;
  }
  if (options.replace) {
    window.history.replaceState(null, "", to);
  } else {
    window.history.pushState(null, "", to);
    window.scrollTo(0, 0);
  }
  notify();
}

export interface Location {
  pathname: string;
  search: URLSearchParams;
}

/** The current location; re-renders when it changes. */
export function useLocation(): Location {
  const href = useSyncExternalStore(subscribe, current);
  const url = new URL(href, window.location.origin);
  return { pathname: url.pathname, search: url.searchParams };
}

/**
 * Matches `pathname` against `pattern` (`/library/:id`): the parameters, or
 * `null`.
 */
export function match(pattern: string, pathname: string): Record<string, string> | null {
  const want = pattern.split("/").filter(Boolean);
  const got = pathname.split("/").filter(Boolean);
  if (want.length !== got.length) {
    return null;
  }
  const params: Record<string, string> = {};
  for (const [index, part] of want.entries()) {
    const value = got[index] ?? "";
    if (part.startsWith(":")) {
      params[part.slice(1)] = decodeURIComponent(value);
    } else if (part !== value) {
      return null;
    }
  }
  return params;
}

/** Replaces query parameters of the current page (empty values remove them). */
export function setSearch(changes: Record<string, string | number | undefined | null>) {
  const url = new URL(window.location.href);
  for (const [key, value] of Object.entries(changes)) {
    if (value === undefined || value === null || value === "") {
      url.searchParams.delete(key);
    } else {
      url.searchParams.set(key, String(value));
    }
  }
  navigate(url.pathname + url.search, { replace: true });
}

type LinkProps = Omit<AnchorHTMLAttributes<HTMLAnchorElement>, "href"> & { to: string };

export function Link({ to, onClick, ...props }: LinkProps) {
  const handle = (event: MouseEvent<HTMLAnchorElement>) => {
    onClick?.(event);
    if (
      event.defaultPrevented ||
      event.button !== 0 ||
      event.metaKey ||
      event.ctrlKey ||
      event.shiftKey ||
      event.altKey
    ) {
      return;
    }
    event.preventDefault();
    navigate(to);
  };
  return <a href={to} onClick={handle} {...props} />;
}
