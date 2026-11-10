# The animated presentation

A two-minute product film about Antumbra, built from code: ten scenes in
HTML and CSS in the wallet's own design language, rendered frame by frame,
with a narrated soundtrack synthesized from the same timeline.

| File | What it is |
|---|---|
| `story.json` | The scenes and the narration, line by line |
| `voice.py` | Synthesizes each line with a neural voice (edge-tts); records its length and when each word is spoken |
| `timeline.py` | Lays the scenes out in time from the measured narration and cuts it into caption cues |
| `deck.html`, `deck.css`, `deck.js` | The scenes. Every animation is paused; the renderer seeks all of them to each frame's time |
| `footage.sh` | Cuts the wallet clips for the product scene from a raw recording of the demo |
| `render.mjs` | Renders the frames in several Chromium processes and pipes them into ffmpeg |
| `music.py` | The soundtrack: pad, arpeggio, sound design on each scene's beats, narration mixed in with ducking |
| `build.sh` | All of the above, in order |

```
cd scripts/video && npm install && npx playwright install chromium
python3 -m venv /tmp/deck-venv && /tmp/deck-venv/bin/pip install numpy scipy edge-tts
PATH=/tmp/deck-venv/bin:$PATH deck/build.sh /tmp/deck docs/demo/antumbra-demo.mp4 /tmp/antumbra-presentation.mp4
```

The film is rendered with the narration burned in as captions, phrase by
phrase and timed to the spoken words, because most people meet a video in
their feed muted; `CAPTIONS=0 deck/build.sh …` leaves them out, and the same
cues are written next to the video as an `.srt`. In `story.json` a line can
carry a pronunciation hint, `{BLS12-381|B L S twelve, three eighty-one}`:
the first form is captioned, the second is spoken.

The raw demo recording comes from `record.mjs` in the parent folder (the
plain cut, without captions). Rendering 3 840 frames at 1920×1080 takes a
few minutes on a laptop; `node deck/render.mjs --stills 12.5,48 --out dir …`
renders single frames for a quick look while editing a scene.

How timing works: a scene lasts as long as its narration plus a lead and a
tail, so changing a sentence in `story.json` moves everything after it.
Elements take their start from CSS variables the timeline sets on each
scene: `--t0` (the scene's start), `--l1`, `--l2`… (where each narration
line begins), so `--d: calc(var(--l2) + .5s)` means "half a second into the
second line". Counters, typing, the footage and the lights are set per
frame in `deck.js`; `music.py` uses the same anchors for its cues. The video
starts `trim` seconds into the timeline, on a frame that is already
composed, so the first frame works as a thumbnail.
