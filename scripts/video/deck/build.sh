#!/bin/bash
# Builds the presentation end to end: narration, timeline, wallet footage,
# picture (frame by frame), soundtrack, final MP4.
#   build.sh <work_dir> [raw_demo.mp4] [out.mp4]      CAPTIONS=0 leaves the burned-in captions out
# Needs node with playwright installed in scripts/video, ffmpeg, and a Python
# with numpy, scipy and edge-tts on PATH (a virtualenv is fine).
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
WORK="${1:?usage: build.sh <work_dir> [raw_demo.mp4] [out.mp4]}"
RAW="${2:-$HERE/../../../docs/demo/antumbra-demo.mp4}"
OUT="${3:-$WORK/antumbra-presentation.mp4}"
mkdir -p "$WORK"
echo "== narration";  python3 "$HERE/voice.py" "$HERE/story.json" "$WORK/voice" | tail -1
echo "== timeline";   python3 "$HERE/timeline.py" "$HERE/story.json" "$WORK/voice" "$WORK/timeline.json" | tail -1
echo "== footage";    [ -d "$WORK/footage/a" ] || "$HERE/footage.sh" "$RAW" "$WORK/footage"
echo "== picture";    node "$HERE/render.mjs" --timeline "$WORK/timeline.json" --footage "$WORK/footage" --video "$WORK/picture.mp4" --workers "${WORKERS:-4}" --crf "${CRF:-16}" --captions "${CAPTIONS:-1}"
echo "== soundtrack"; python3 "$HERE/music.py" "$WORK/timeline.json" "$WORK/voice" "$WORK/audio.wav" | tail -3
echo "== mux"
ffmpeg -y -loglevel error -i "$WORK/picture.mp4" -i "$WORK/audio.wav" -c:v copy -af "loudnorm=I=-15:TP=-1.5:LRA=11,aresample=48000" -c:a aac -b:a 192k -shortest -movflags +faststart "$OUT"
ffprobe -v error -show_entries format=duration,size:stream=codec_name,width,height,r_frame_rate,sample_rate -of default=nw=1 "$OUT"
cp "$WORK/timeline.srt" "${OUT%.mp4}.srt"
echo "done: $OUT (captions also in ${OUT%.mp4}.srt)"
