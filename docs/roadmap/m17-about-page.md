# M17 – About page

**Depends on:** M5 (the web UI), M13 (roles), M16 (the status bar)

## Why

To report a problem, or to check an upgrade went through, one needs to know which build runs, on what machine, with which features on. Today the pieces are scattered: the version in a banner shown only when the web UI and the API differ, the services' versions behind `/healthz`, the configuration in environment variables only the person who deployed can read. One page gathers them, and a button copies them for a bug report.

## Scope

- **An About page**, linked at the bottom of the navigation next to Account, at `/about`.
- **Build** (every role): the Heron version, the commit it was built from, whether the web UI and the API come from the same build, the version of each service (API, engine, analyzer, librarian) with whether it answers, the build date.
- **Runtime** (admins): how long each service has been running, the operating system, kernel and architecture of the machine, whether Heron runs in a container, the time zone and the clock's synchronization, the CPU count and memory.
- **Dependencies** (admins): the database server and its version, the applied migrations, the FFmpeg version when the external encoder is set (`built-in` otherwise), the version of each stream server when it tells.
- **Active configuration** (admins, read-only): which features are on (library and incoming folder watching, error reporting, metrics remote write, external FFmpeg, the monitor output, the number of stations) and the values that are not secret (folders, intervals, worker counts, listen addresses). A secret shows only as *set* or *not set*.
- **Links:** documentation, changelog, repository, license.
- **Copy diagnostics:** one button puts everything the user may see on the clipboard as plain text, to paste into a bug report.
- **One route**, `GET /api/v1/about`, answers with the build part to every role and the other parts to admins only.

## Out of scope

- Changing any value: the page only reads. Settings stay where they are.
- History of versions or of upgrades.
- Checking for a newer release.
- Disk space, temperature and per-process figures: see [Monitoring](../monitoring.md#what-else-could-be-monitored).

## Deliverables

- `heron-api`: `GET /api/v1/about`, aggregating the services' own reports; a field-by-field list of the configuration shown (never a list of what is hidden).
- Every service: uptime and system details in its `/healthz`.
- `heron-web`: the page, the navigation link, translations (all catalogs), the copy button.
- Docs: [Web UI](../web.md#pages), [API](../api.md#routes).

## Acceptance criteria

- [ ] The page is linked at the bottom of the navigation and works from a phone up, in light and dark themes.
- [ ] A viewer or operator receives and sees the build part and the links only; an admin sees every part.
- [ ] A secret (database URL, error-reporting DSN, metrics credentials, and any added later) never appears in the answer, whatever the configuration: a test sets them and searches the answer.
- [ ] With a service stopped, the page says so for that service and still shows the others.
- [ ] The web UI and the API from different builds show as different, as the banner does.
- [ ] *Copy diagnostics* copies what the user sees, and nothing more.
- [ ] Unit tests for the route and its roles, the configuration list and the page; docs, translations (all catalogs) and screenshots are up to date.

## References

- [Web UI – Status bar](../web.md#status-bar)
- [API – Authentication](../api.md#authentication)
- [Monitoring](../monitoring.md)
