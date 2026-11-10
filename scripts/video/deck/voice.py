#!/usr/bin/env python3
"""Synthesizes every narration line of story.json with a neural voice
(edge-tts) and records, next to the audio, how long each line is and when
each word is spoken, so the captions can follow the voice. A line may carry
pronunciation hints as {shown|spoken}: "{BLS12-381|B L S twelve, three
eighty-one}" is read the second way and captioned the first. The service
throttles, so each line is retried with a growing pause; lines already
synthesized for the same text are kept.
Usage: voice.py story.json out_dir"""
import asyncio, hashlib, json, re, subprocess, sys, time, pathlib
import edge_tts

story = json.load(open(sys.argv[1])); out = pathlib.Path(sys.argv[2]); out.mkdir(parents=True, exist_ok=True)
spoken = lambda line: re.sub(r'\{([^|}]*)\|([^}]*)\}', r'\2', line)

async def synth(text, mp3):
    com = edge_tts.Communicate(text, story['voice'], rate=story['rate'], boundary='WordBoundary')
    words = []
    with open(mp3, 'wb') as f:
        async for ch in com.stream():
            if ch['type'] == 'audio': f.write(ch['data'])
            elif ch['type'] == 'WordBoundary': words.append({'t': round(ch['offset'] / 1e7, 3), 'd': round(ch['duration'] / 1e7, 3), 'w': ch['text']})
    return words

durs, words = {}, {}
for sc in story['scenes']:
    for i, line in enumerate(sc['lines']):
        key = f"{sc['id']}-{i + 1}"; text = spoken(line)
        mp3, meta = out / f'{key}.mp3', out / f'{key}.json'
        want = hashlib.sha256(f"{story['voice']}|{story['rate']}|{text}".encode()).hexdigest()
        cached = json.loads(meta.read_text()) if meta.exists() else {}
        if not (mp3.exists() and mp3.stat().st_size > 2000 and cached.get('hash') == want and cached.get('words')):
            for attempt in range(8):
                try:
                    w = asyncio.run(synth(text, mp3))
                    if w and mp3.stat().st_size > 2000:
                        cached = {'hash': want, 'words': w}; meta.write_text(json.dumps(cached)); break
                except Exception as e:  # throttled or a dropped connection: wait and try again
                    err = e
                time.sleep(2 + 3 * attempt)
            else:
                sys.exit(f'could not synthesize {key}: {err}')
            time.sleep(1.0)
        d = subprocess.run(['ffprobe', '-v', 'error', '-show_entries', 'format=duration', '-of', 'csv=p=0', str(mp3)], capture_output=True, text=True).stdout.strip()
        durs[key] = round(float(d), 3); words[key] = cached['words']
        print(f'{key:12s} {durs[key]:6.2f}s {len(words[key]):3d} words  {text[:60]}', flush=True)
json.dump(durs, open(out / 'durations.json', 'w'), indent=1)
json.dump(words, open(out / 'words.json', 'w'))
print('total speech', round(sum(durs.values()), 1), 's')
