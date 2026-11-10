#!/usr/bin/env bash
# usage: gen-ogg.sh <out-dir>
# The Ogg Vorbis files gmnb-sound's tests read: made by ffmpeg from its own
# sine source, written to files only (no sound device is opened).
set -eu
out=$1
mkdir -p "$out"
ff() { ffmpeg -hide_banner -loglevel error -y -threads 1 "$@" -c:a libvorbis -map_metadata -1 -fflags +bitexact -flags:a +bitexact; }
ff -f lavfi -i "sine=frequency=440:sample_rate=44100:duration=0.25" -ac 1 -q:a 3 "$out/tone-mono-44100.ogg"
ff -f lavfi -i "sine=frequency=660:sample_rate=48000:duration=0.1" -ac 2 -q:a 3 "$out/tone-stereo-48000.ogg"
ff -f lavfi -i "sine=frequency=220:sample_rate=8000:duration=4.5" -ac 1 -q:a 0 "$out/long-mono-8000.ogg"
ff -f lavfi -i "sine=frequency=330:sample_rate=22050:duration=0.1" -ac 3 -q:a 0 "$out/three-channels.ogg"
ls -l "$out"
for f in "$out"/*.ogg; do
  ffprobe -hide_banner -loglevel error -show_entries stream=codec_name,channels,sample_rate,duration_ts -of csv=p=0 "$f"
done
