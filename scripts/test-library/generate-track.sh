#!/usr/bin/env sh
# SPDX-License-Identifier: AGPL-3.0-or-later
#
# Generates test track number $1 under /out. Runs inside the ffmpeg
# container started by scripts/generate-test-library.sh.

set -eu

i=$1
artist=$(( (i - 1) / 12 + 1 ))
album=$(( (i - 1) / 4 % 3 + 1 ))
number=$(( (i - 1) % 4 + 1 ))
case $(( i % 3 )) in
    0) ext=mp3;  set -- -c:a libmp3lame -b:a 128k ;;
    1) ext=m4a;  set -- -c:a aac -b:a 128k ;;
    *) ext=flac; set -- -c:a flac ;;
esac
folder="/out/Artist $artist/Album $album"
file="$folder/$(printf %02d "$number") Track $i.$ext"
[ -f "$file" ] && exit 0
mkdir -p "$folder"

lead=$(( i % 4 ))           # 0-3 s of silence before the music
tail=$(( (i * 7) % 5 ))     # 0-4 s after
music=$(( 20 + i % 15 ))    # 20-34 s of music
freq=$(( 220 + (i * 37) % 660 ))
# Every 5th MP3 carries several MB of cover art in its ID3v2 tag, like many
# real files: decoders must skip it.
if [ "$ext" = mp3 ] && [ $(( i % 5 )) -eq 0 ]; then
    set -- -f lavfi -i "nullsrc=s=1200x1200,geq=random(1)*255:128:128" \
        -frames:v 1 -map 3:v -c:v png -disposition:v attached_pic -id3v2_version 3 "$@"
fi

# Write to a temporary name and rename, so the scanner never sees half a file.
tmp="$folder/.tmp-$i.$ext"
ffmpeg -loglevel error -nostdin -y \
    -f lavfi -i "anullsrc=r=44100:cl=stereo:d=$lead" \
    -f lavfi -i "sine=f=$freq:r=44100:d=$music" \
    -f lavfi -i "anullsrc=r=44100:cl=stereo:d=$tail" \
    "$@" \
    -filter_complex "[1]volume=0.5,aformat=channel_layouts=stereo[m];[0][m][2]concat=n=3:v=0:a=1[a]" \
    -map "[a]" \
    -metadata artist="Artist $artist" -metadata album="Album $album" \
    -metadata title="Track $i" -metadata track="$number" \
    "$tmp"
mv "$tmp" "$file"
