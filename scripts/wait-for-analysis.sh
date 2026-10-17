#!/usr/bin/env sh
# SPDX-License-Identifier: AGPL-3.0-or-later
#
# Waits until the library status reports <tracks> analyzed tracks:
#   scripts/wait-for-analysis.sh <base-url> <tracks> <timeout-seconds>

set -eu

url=${1:?usage: wait-for-analysis.sh <base-url> <tracks> <timeout-seconds>}
expected=${2:?}
timeout=${3:?}
start=$(date +%s)

while :; do
    status=$(curl -fsS "$url/api/v1/library/status" || echo '{}')
    analyzed=$(echo "$status" | sed -n 's/.*"analyzed":\([0-9]*\).*/\1/p')
    tracks=$(echo "$status" | sed -n 's/^{"tracks":\([0-9]*\).*/\1/p')
    elapsed=$(( $(date +%s) - start ))
    echo "${elapsed}s: ${tracks:-?} tracks, ${analyzed:-?} analyzed"
    if [ "${tracks:-0}" -ge "$expected" ] && [ "${analyzed:-0}" -ge "$expected" ]; then
        echo "$status"
        exit 0
    fi
    if [ "$elapsed" -ge "$timeout" ]; then
        echo "Timed out. Last status: $status"
        exit 1
    fi
    sleep 5
done
