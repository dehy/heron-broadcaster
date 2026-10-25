#!/usr/bin/env sh
# SPDX-License-Identifier: AGPL-3.0-or-later
#
# Feeds an Icecast fallback mount with the first MP3 file of a directory,
# looped in real time and copied without re-encoding, so it stays in the
# file's format: encode it like the stream (MP3 128k:
# `-c:a libmp3lame -b:a 128k -ar 44100 -ac 2`). Without a file, plays
# silence in that format.
#
#   fallback-source.sh <directory> <icecast://source:password@host:port/mount>

set -eu

dir=$1
target=$2

file=$(find "$dir" -maxdepth 1 -type f -name '*.mp3' -size +0 | sort | head -n 1)
if [ -z "$file" ]; then
    echo "no MP3 file in $dir: streaming silence (MP3 128k)" >&2
    file=/tmp/fallback-silence.mp3
    ffmpeg -hide_banner -loglevel error -y -f lavfi -i anullsrc=r=44100:cl=stereo -t 30 \
        -c:a libmp3lame -b:a 128k -ar 44100 -ac 2 "$file"
fi

exec ffmpeg -hide_banner -loglevel warning -re -stream_loop -1 -i "$file" -c copy -f mp3 \
    -content_type audio/mpeg -ice_name "fallback" "$target"
