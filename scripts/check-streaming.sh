#!/usr/bin/env sh
# SPDX-License-Identifier: AGPL-3.0-or-later
#
# Checks that the engine of a running Compose stack streams to Icecast:
# configures an output to the `icecast` service, queues three tracks of the
# library and listens to the stream.
#
#   docker compose --profile icecast up -d --wait
#   scripts/check-streaming.sh <music dir> [icecast url]

set -eu

music=${1:?usage: check-streaming.sh <music dir> [icecast url]}
icecast=${2:-http://localhost:${HERON_ICECAST_PORT:-8010}}
ctl="docker compose exec -T heron-engine heron-engine ctl"

$ctl set-outputs /dev/stdin <<'EOF'
{
  "encoder_profiles": [
    {"id": 1, "name": "MP3 128k", "encoder": "mp3", "bitrate_kbps": 128, "sample_rate": 44100, "channels": 2}
  ],
  "outputs": [
    {"id": 1, "name": "local", "kind": "icecast", "encoder_profile_id": 1,
     "config": {"host": "icecast", "port": 8000, "mount": "/live", "password": "hackme"}}
  ]
}
EOF

# Paths relative to the library, as the engine sees it.
(cd "$music" && find . -type f \( -name '*.mp3' -o -name '*.m4a' -o -name '*.flac' \) | sort | head -3 | sed 's|^\./||') |
    tr '\n' '\0' | xargs -0 $ctl enqueue

start=$(date +%s)
until curl -fsS "$icecast/status-json.xsl" | grep -q '"title"'; do
    if [ $(( $(date +%s) - start )) -ge 30 ]; then
        echo "no stream with a title on $icecast after 30 s" >&2
        $ctl state >&2 || true
        exit 1
    fi
    sleep 1
done
curl -fsS "$icecast/status-json.xsl" | tr ',' '\n' | grep -E '"(title|audio_info|bitrate)"'

# 5 s of a 128 kbps stream is 80 kB, plus the server's burst.
bytes=$(curl -s -m 5 "$icecast/live" | wc -c)
echo "received $bytes bytes in 5 s"
[ "$bytes" -ge 70000 ]
$ctl state
