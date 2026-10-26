# Upgrading

How to move a station to a new version of Heron without taking it off air, with Docker Compose or Docker Swarm. Built in [M8](roadmap/m8-hot-engine-upgrade.md).


## What an upgrade touches

| Service | Upgrading it on air |
|---|---|
| `heron-web`, `heron-api`, `heron-librarian`, `heron-analyzer` | At any time: the engine plays without them (queue in memory, then the fallback directory). The web interface reconnects; `heron-api` runs the database migrations when it starts. |
| `heron-engine` | Off air for a few seconds when it is simply restarted, and Icecast disconnects the listeners when its source leaves. Without a gap with the **hot handover**: the new engine takes over from the old one ([Audio engine – Upgrades and handover](audio-engine.md#upgrades-and-handover)). |
| `heron-relay` | Holds the engine's connections to the stream servers, so they outlive the engine's container ([Engine handover](handover.md#why-the-connections-go-through-a-relay)). It only carries bytes and almost never changes: `scripts/upgrade.sh` leaves it running. Restart it (`docker compose up -d heron-relay`) when a release says so; it is a short break covered by the fallback mount. |
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
4. waits for the standby to be healthy, then stops the old engine: the old engine hands over to the new one (its connections to `heron-relay`) and exits;
5. removes the old container, and checks that one engine is on air.

If the standby does not become healthy within 2 min, the script removes it, leaves the running engine alone, and fails: the station stays on air on the old version.

With images built locally (`docker compose build`), add `--no-pull`. The handover happens even when the engine's image did not change: it is harmless, and a way to try it.

A plain `docker compose pull && docker compose up -d` also works, but Compose stops the old engine before starting the new one: a cold restart of a few seconds, which the fallback mount covers.

To go back to the previous version, run the script with it. Migrations are not undone: going back across a migration needs the database backup.

## Docker Swarm

A Swarm stack is deployed with `docker stack deploy` from a directory of its own. Keep there everything the stack mounts, and write its paths relative to the stack file (`docker stack deploy` resolves them from the file's directory):

```
docker-stack.yml           the stack
.env                       its settings (versions, passwords, folders)
icecast/icecast.xml        copied from docker/icecast/ in the repository
icecast/fallback-source.sh
data/                      the incoming, fallback and fallback-stream folders, unless .env points elsewhere
```

`docker stack deploy` does not read `.env` itself: load it into the shell first.

```sh
cd /path/to/the/stack
set -a; . ./.env; set +a
docker stack deploy -c docker-stack.yml heron
```

Swarm has no Compose profiles (`profiles:` is refused): to deploy the stack with or without its own Icecast, put `icecast` and `icecast-fallback` in a second file and add it with another `-c`.

### What the stack needs

A Swarm stack can start the new engine before stopping the old one (`start-first`). Give `heron-engine` this, and add `heron-relay` and its volume:

```yaml
  heron-engine:
    image: ${HERON_REGISTRY:-ghcr.io/dehy}/heron-engine:${HERON_VERSION:-latest}
    stop_grace_period: 30s
    environment:
      # …the other variables
      HERON_RELAY_SOCKET: /run/heron/relay/relay.sock
    volumes:
      # …the other volumes
      - engine-state:/var/lib/heron/engine
      - engine-relay:/run/heron/relay
    deploy:
      replicas: 1
      update_config:
        order: start-first
        failure_action: rollback
      restart_policy:
        condition: any
        delay: 10s

  heron-relay:
    image: ${HERON_REGISTRY:-ghcr.io/dehy}/heron-engine:${HERON_RELAY_VERSION:-latest}
    command: ["relay"]
    environment:
      HERON_RELAY_SOCKET: /run/heron/relay/relay.sock
    volumes:
      - engine-relay:/run/heron/relay
    healthcheck:
      test: ["CMD", "heron-engine", "relay", "--check"]
      interval: 10s
    deploy:
      replicas: 1
      restart_policy:
        condition: any

volumes:
  engine-relay:
```

And Icecast with its fallback mount, from the files next to the stack:

```yaml
  icecast:
    image: libretime/icecast:2.4.4
    # The image edits its configuration in place: start from a copy of ours.
    entrypoint: ["sh", "-c", "cp /etc/heron/icecast.xml /etc/icecast.xml && exec docker-entrypoint \"$$@\"", "--"]
    command: ["icecast", "-c", "/etc/icecast.xml"]
    environment:
      ICECAST_SOURCE_PASSWORD: ${HERON_ICECAST_SOURCE_PASSWORD}
      ICECAST_RELAY_PASSWORD: ${HERON_ICECAST_ADMIN_PASSWORD}
      ICECAST_ADMIN_PASSWORD: ${HERON_ICECAST_ADMIN_PASSWORD}
    volumes:
      - ./icecast/icecast.xml:/etc/heron/icecast.xml:ro
    ports:
      - "8000:8000"

  icecast-fallback:
    image: linuxserver/ffmpeg:7.1.1
    entrypoint: ["/fallback-source.sh"]
    command: ["/fallback", "icecast://source:${HERON_ICECAST_SOURCE_PASSWORD}@icecast:8000/fallback"]
    volumes:
      - ./icecast/fallback-source.sh:/fallback-source.sh:ro
      - ./data/fallback-stream:/fallback:ro
```

Swarm refuses to start a task whose bind-mounted folder does not exist: create the `data/` folders before deploying.

`heron-engine`, `heron-relay` and their volumes must stay on one node (the volumes are local, and the handover passes connections between processes): pin them with a placement constraint on a multi-node swarm. The relay must be on the network the stream servers are reached from: it connects to them, not the engine.

### Upgrading

Upgrade with the script, giving the stack's name:

```sh
scripts/upgrade.sh --swarm heron 0.2.0
```

It pulls the images on the node first, so the update does not wait for the download, then runs `docker service update --image …` for each Heron service but the relay, the engine last, with `start-first`, a rollback on failure and a 30 s stop grace period (set even if the stack file lacks them). The engine is always updated (`--force`), even when its image did not change. Swarm starts the new engine, waits for it to be healthy (the standby), then stops the old one, which hands over.

Deploying the stack again (`docker stack deploy`, after changing `HERON_VERSION` in `.env`) upgrades it the same way, `start-first`, with one difference: `docker stack deploy` checks every image again, so it also replaces `heron-relay` whenever its image changed, a short break covered by the fallback mount. To leave the relay running, give it a version of its own in `.env` (`HERON_RELAY_VERSION`), changed only when a release says so.

## Checking an upgrade

```sh
docker compose exec heron-engine heron-engine ctl state     # the track on air, the queue
curl -s http://localhost:8010/status-json.xsl               # one source on /live, the listeners
```

`/healthz` of the engine reports `"role": "active"`; a `"standby"` engine left behind means a handover did not happen (see the logs of both engines). The Grafana dashboard ([Monitoring](monitoring.md)) shows the version of each service.

A new engine can only take over from an engine that speaks the same handover protocol. When a release changes it, its release notes say so, and the upgrade is a cold restart covered by the fallback mount.
