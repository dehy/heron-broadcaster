#!/usr/bin/env sh
# SPDX-License-Identifier: AGPL-3.0-or-later
#
# Upgrades a running station without taking it off air: every service, the
# engine last, by a hot handover from the running engine to a new one
# (docs/upgrading.md, docs/handover.md).
#
#   scripts/upgrade.sh [--no-pull] [version]                # Docker Compose
#   scripts/upgrade.sh --swarm <stack> [--no-pull] [version] # Docker Swarm
#
# With a version, Compose's .env gets HERON_VERSION=<version>; in Swarm the
# Heron images of the stack get that tag. Run from the directory of
# docker-compose.yml.

set -eu

swarm=
pull=yes
version=
while [ $# -gt 0 ]; do
    case $1 in
    --swarm)
        swarm=${2:?--swarm needs the name of the stack}
        shift 2
        ;;
    --no-pull)
        pull=
        shift
        ;;
    -h | --help)
        sed -n '4,13p' "$0"
        exit 0
        ;;
    -*)
        echo "unknown option $1" >&2
        exit 2
        ;;
    *)
        version=$1
        shift
        ;;
    esac
done

say() {
    echo "==> $*"
}

fail() {
    echo "upgrade failed: $*" >&2
    exit 1
}

# Docker Swarm: start-first updates of the engine do the handover.
if [ -n "$swarm" ]; then
    engine=
    for service in $(docker stack services --format '{{.Name}}' "$swarm"); do
        image=$(docker service inspect -f '{{.Spec.TaskTemplate.ContainerSpec.Image}}' "$service")
        image=${image%%@*}
        case $image in
        ghcr.io/dehy/heron-* | heron-*) ;;
        *) continue ;;
        esac
        if [ -n "$version" ]; then
            image="${image%:*}:$version"
        fi
        if [ -n "$pull" ]; then
            say "pulling $image"
            docker pull --quiet "$image" >/dev/null
        fi
        case $service in
        *heron-engine) engine="$service $image" ;;
        # Holds the connections: left running (docs/upgrading.md).
        *heron-relay) ;;
        *)
            say "updating $service"
            docker service update --quiet --with-registry-auth --image "$image" "$service" >/dev/null
            ;;
        esac
    done
    [ -n "$engine" ] || fail "no heron-engine service in stack $swarm"
    set -- $engine
    say "updating $1: the new engine takes over from the running one"
    docker service update --quiet --with-registry-auth --image "$2" \
        --update-order start-first --update-failure-action rollback \
        --stop-grace-period 30s --force "$1" >/dev/null
    say "done"
    exit 0
fi

# Docker Compose.
compose() {
    docker compose "$@"
}

if [ -n "$version" ]; then
    say "HERON_VERSION=$version in .env"
    if [ -f .env ] && grep -q '^HERON_VERSION=' .env; then
        sed "s/^HERON_VERSION=.*/HERON_VERSION=$version/" .env >.env.upgrade
        mv .env.upgrade .env
    else
        echo "HERON_VERSION=$version" >>.env
    fi
fi

if [ -n "$pull" ]; then
    say "pulling the images"
    compose pull --quiet
fi

# heron-relay holds the connections to the stream servers: left running.
others=$(compose ps --services --status running | grep -v -e '^heron-engine$' -e '^heron-relay$' || true)
if [ -n "$others" ]; then
    say "upgrading $(echo "$others" | tr '\n' ' ')"
    # shellcheck disable=SC2086
    compose up -d --no-deps --wait $others
fi

old=$(compose ps -q heron-engine)
if [ -z "$old" ]; then
    say "no engine running: starting one"
    compose up -d --no-deps --wait heron-engine
    exit 0
fi
[ "$(echo "$old" | wc -l)" -eq 1 ] || fail "more than one heron-engine container runs"

say "starting the new engine next to the running one"
compose up -d --no-deps --no-recreate --scale heron-engine=2 heron-engine
new=$(compose ps -q heron-engine | grep -v "^$old$" || true)
[ -n "$new" ] || fail "the new engine did not start"

waited=0
until [ "$(docker inspect -f '{{.State.Health.Status}}' "$new")" = healthy ]; do
    if [ "$waited" -ge 120 ] || [ "$(docker inspect -f '{{.State.Running}}' "$new")" != true ]; then
        docker logs --tail 20 "$new" >&2 || true
        docker rm -f "$new" >/dev/null
        fail "the new engine is not healthy: the running engine stays on air"
    fi
    sleep 2
    waited=$((waited + 2))
done

say "handing over to the new engine"
docker stop --time 30 "$old" >/dev/null
docker rm "$old" >/dev/null

waited=0
until compose exec -T heron-engine heron-engine ctl state >/dev/null 2>&1; do
    [ "$waited" -lt 30 ] || fail "the new engine does not answer"
    sleep 1
    waited=$((waited + 1))
done
say "done: $(docker inspect -f '{{.Config.Image}}' "$new") on air"
