// SPDX-License-Identifier: AGPL-3.0-or-later

// Error reporting to Sentry, off unless the heron-web container has
// HERON_WEB_SENTRY_DSN set (served in /config.js). The SDK is loaded only
// then, so it costs nothing otherwise.

interface HeronConfig {
  sentryDsn?: string;
  sentryEnvironment?: string;
}

declare global {
  interface Window {
    heronConfig?: HeronConfig;
  }
  /** The web app's version, from package.json. */
  const __APP_VERSION__: string;
}

export function startErrorReporting(): void {
  const config = window.heronConfig ?? {};
  const dsn = config.sentryDsn?.trim();
  if (!dsn) {
    return;
  }
  void import("@sentry/react").then((Sentry) => {
    Sentry.init({
      dsn,
      environment: config.sentryEnvironment || "production",
      release: `heron-web@${__APP_VERSION__}`,
      // No user details, cookies, headers or bodies: sessions and API
      // tokens travel in them.
      dataCollection: {
        userInfo: false,
        cookies: false,
        httpHeaders: false,
        httpBodies: [],
        urlQueryParams: false,
      },
    });
    Sentry.setTag("service", "heron-web");
  });
}
