#!/usr/bin/env python3
"""Synthesizes the narration lines with a neural voice (edge-tts) and records
their durations for the composer. Usage: voice.py narration.json out_dir
(run inside a virtualenv with edge-tts installed)."""
import json, subprocess, sys, pathlib
script, out = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2])
voice = sys.argv[3] if len(sys.argv) > 3 else 'en-US-AndrewNeural'
out.mkdir(parents=True, exist_ok=True)
durs = {}
for name, line in json.load(open(script)).items():
    mp3 = out / f'{name}.mp3'
    subprocess.run(['edge-tts', '--voice', voice, '--rate=-4%', '--text', line['text'], '--write-media', str(mp3)], check=True)
    durs[name] = round(float(subprocess.run(['ffprobe', '-v', 'error', '-show_entries', 'format=duration', '-of', 'csv=p=0', str(mp3)], capture_output=True, text=True).stdout.strip()), 2)
json.dump(durs, open(out / 'durations.json', 'w'), indent=1)
print(durs)
