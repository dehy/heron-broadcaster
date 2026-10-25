# Upgrading

How to move a station to a new version of Heron without taking it off air, with Docker Compose or Docker Swarm. Built in [M8](roadmap/m8-hot-engine-upgrade.md).

> **Not built yet.** Until M8 is done, `scripts/upgrade.sh`, the hot handover and the `icecast-fallback` service do not exist: upgrade with `docker compose pull && docker compose up -d`, and expect the engine to be off air for a few seconds.

## What an upgrade touches

| Service | Upgrading it on air |
|---|---|
| `heron-web`, `heron-api`, `heron-librarian`, `heron-analyzer` | At any time: the engine plays without them (queue in memory, then the fallback directory). The web interface reconnects; `heron-api` runs the database migrations when it starts. |
| `heron-engine` | Off air for a few seconds when it is simply restarted, and Icecast disconnects the listeners when its source leaves. Without a gap with the **hot handover**: the new engine takes over from the old one ([Audio engine – Upgrades and handover](audio-engine.md#upgrades-and-handover)). |
| `mariadb`, `icecast` | Not covered here. Restarting Icecast disconnects the listeners; the engine reconnects to it. |

Two things keep the listeners: the hot handover (no gap at all), and an Icecast **fallback mount** for the times the engine does restart (a crash, a host reboot, an upgrade done the plain way). Set up both.

Before upgrading: read the release notes, and back up the database (migrations only go forward):

```sh
docker compose exec mariadb mariadb-dump -u root -p heron > heron-$(date +%F).sql
```

## Icecast fallback mount

When the engine's source leaves a mount, Icecast moves its listeners to the mount's **fallback**; with `fallback-override`, it moves them back as soon as the source returns. Listeners hear the fallback for a few seconds instead of losing the stream.

The fallback is fed by a small `icecast-fallback` service that loops one file, with `ffmpeg -re -stream_loop -1 -c copy` (no re-encoding). The `icecast` profile of `docker-compose.yml` has both, with `docker/icecast/icecast.xml`:

```xml
<mount type="normal">
  <mount-name>/live</mount-name>
  <fallback-mount>/fallback</fallback-mount>
  <fallback-override>1</fallback-override>
</mount>
<mount type="normal">
  <mount-name>/fallback</mount-name>
  <hidden>1</hidden>
</mount>
```

The fallback file **must be in the stream's format**: the same codec, sample rate, channels and bitrate as the output's encoder profile (MP3, 44.1 kHz, stereo, 128 kbps CBR for `MP3 128k`). Players do not expect the format to change mid-stream, and some stop. Use a station ID or a short bed, encoded with the same settings:

```sh
ffmpeg -i station-id.wav -c:a libmp3lame -b:a 128k -ar 44100 -ac 2 data/fallback-stream.mp3
```

Each mount the engine streams to needs its own fallback in the same format: one `icecast-fallback` per encoder profile.

With your own Icecast server, add the two `<mount>` blocks to its configuration and run the looping source next to it. Icecast can also fall back to a file in its webroot (`<fallback-mount>/fallback.mp3</fallback-mount>`), without a source to run, but it serves the file faster than real time, and listeners have been reported to wait over a minute before hearing the station again. Prefer the mount.

## Docker Compose

Upgrade with the script, from the directory of `docker-compose.yml`:

```sh
scripts/upgrade.sh 0.2.0    # or without a version, to pull the tag already set
```

It sets `HERON_VERSION=0.2.0` in `.env`, then:

1. pulls every image;
2. upgrades every service except the engine (`docker compose up -d --no-deps --wait`);
3. starts a second `heron-engine` container from the new image, next to the running one: it becomes the standby;
4. waits for the standby to be healthy, then stops the old engine: the old engine hands over to the new one and exits;
5. removes the old container, and checks that one engine is on air.

If the standby does not become healthy within 2 min, the script removes it, leaves the running engine alone, and fails: the station stays on air on the old version.

A plain `docker compose pull && docker compose up -d` also works, but Compose stops the old engine before starting the new one: a cold restart of a few seconds, which the fallback mount covers.

To go back to the previous version, run the script with it. Migrations are not undone: going back across a migration needs the database backup.

## Docker Swarm

A Swarm stack can start the new engine before stopping the old one (`start-first`). Give `heron-engine` this in the stack file:

```yaml
  heron-engine:
    image: ghcr.io/dehy/heron-engine:${HERON_VERSION:-latest}
    stop_grace_period: 30s
    deploy:
      replicas: 1
      update_config:
        order: start-first
        failure_action: rollback
      restart_policy:
        condition: any
        delay: 10s
```

`heron-engine` and its `engine-state` volume must stay on one node (the volume is local, and the handover passes connections between processes): pin it with a placement constraint on a multi-node swarm.

Then upgrade with the script, giving the stack's name:

```sh
scripts/upgrade.sh --swarm heron 0.2.0
```

It pulls the images on the node first, so the update does not wait for the download, then runs `docker service update --image …` for each service, the engine last. Swarm starts the new engine, waits for it to be healthy (the standby), then stops the old one, which hands over.

From Portainer, update the stack's `HERON_VERSION` and redeploy it with *Re-pull image*: the `start-first` update works the same way.

## Checking an upgrade

```sh
docker compose exec heron-engine heron-engine ctl state     # the track on air, the queue
curl -s http://localhost:8010/status-json.xsl               # one source on /live, the listeners
```

`/healthz` of the engine reports `"role": "active"`; a `"standby"` engine left behind means a handover did not happen (see the logs of both engines). The Grafana dashboard ([Monitoring](monitoring.md)) shows the version of each service.

A new engine can only take over from an engine that speaks the same handover protocol. When a release changes it, its release notes say so, and the upgrade is a cold restart covered by the fallback mount.
