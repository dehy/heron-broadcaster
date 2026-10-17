# Overview

## What we are building

A modern, self-hosted **radio automation system** for Linux, replacing dated Windows tools like SAM Broadcaster. It:

- plays music 24/7 to a streaming server (Icecast first);
- schedules tracks with category-based clockwheels;
- crossfades between tracks and removes silences;
- processes audio through DSP plugins (Stereo Tool first);
- is fully operated from a web interface.

## Identity

| Item | Value |
|---|---|
| Project name | **Heron Broadcaster** ("Heron" for short in UI and docs) |
| Domain | heronbroadcaster.org |
| Git repository | `heron-broadcaster` |
| License | **AGPL-3.0** |
| Crate prefix | `heron-` (directory `crates/engine` → package `heron-engine`) |
| Binaries | `heron-engine`, `heron-api`, `heron-analyzer` |
| Docker images | `heron-engine`, `heron-api`, `heron-analyzer`, `heron-web` |

The name comes from "air on" sounding like *héron* (French for heron): keeping the air on, 24/7.

## See also

- [Locked decisions](decisions.md)
- [Architecture](architecture.md)
- [Requirements](requirements.md)
