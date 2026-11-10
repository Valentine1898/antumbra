#!/bin/bash
# Cuts the wallet clips for the product scene out of a raw recording of the
# demo flow (docs/demo/antumbra-demo.mp4: 1440x860, no captions except during
# the sped-up wait, which these cuts avoid) as frame sequences at the speed
# they play in the deck.   footage.sh <raw.mp4> <out_dir>
set -euo pipefail
SRC="$1"; OUT="$2"
cut() { # id  source_start  source_length  playback_seconds
  mkdir -p "$OUT/$1"; rm -f "$OUT/$1"/*.jpg
  ffmpeg -y -loglevel error -ss "$2" -t "$3" -i "$SRC" -vf "setpts=PTS*$4/$3,fps=30,crop=1440:810:0:0,scale=1280:720:flags=lanczos" -q:v 2 "$OUT/$1/%04d.jpg"
  echo "$1: $(ls "$OUT/$1" | wc -l | tr -d ' ') frames"
}
cut a  21.0  9.0  3.7    # shield 0.5 wNEAR
cut b  34.4 10.2  3.8    # swap page, live quote, seal and swap
cut c1 50.6  8.8  2.3    # sealed card, what the chain saw
cut c2 68.8  4.6  2.3    # cleared and claimed
cut d  83.2 15.0  3.25   # unshield
