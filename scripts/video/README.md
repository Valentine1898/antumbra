# Recording the demo video

Everything here drives the real web wallet against the testnet pool and turns
the recording into the product video: intro card, step captions, a sped-up
wait for the sealer, outro card. Needs Node 22, `ffmpeg`, and the sealer
daemon running for the pool (`antumbra sealer watch --pair 1`).

```
cd scripts/video && npm install && npx playwright install chromium
cd ../../web && npm run build && npx vite preview --port 4173 &     # a static build, no HMR client

# a funded throwaway gas payer; the key goes to the file, never to the terminal
OUT=/tmp/demo-account.secret.json node make-demo-account.mjs

# overlays in the app's own design: intro, outro, eight captions
CAPTIONS="$(cat captions.json)" OUT=/tmp/overlays W=1920 H=1080 node overlays.mjs

# the take: wrap, mint, shield, sealed swap with a live NEAR Intents quote, sealer, auto-claim, unshield
S=/tmp URL=http://localhost:4173/ ACCOUNT_FILE=/tmp/demo-account.secret.json VIDEO_DIR=/tmp/take W=1920 H=1080 node record.mjs

# compose: H.264 1080p30 with a silent track, under X's limit
python3 compose.py /tmp/take /tmp/overlays /tmp/antumbra-product.mp4

# the product cut: animated cards (cards.mjs), a neural voice-over (narration.json through
# edge-tts in a virtualenv), camera moves on key moments, a quiet synthesized bed ducked under the voice
OUT=/tmp/cards W=1920 H=1080 node cards.mjs
python3 -m venv /tmp/tts && /tmp/tts/bin/pip install edge-tts && PATH=/tmp/tts/bin:$PATH python3 voice.py narration.json /tmp/voice
python3 compose2.py /tmp/take /tmp/overlays /tmp/cards /tmp/voice /tmp/antumbra-product.mp4
```

`deck/` holds a different kind of video: the animated presentation, with
diagrams instead of a screen recording; see `deck/README.md`.

`record.mjs` seeds the gas payer into localStorage so the video opens
connected, draws a cursor (headless video has none), and writes
`markers.json` with the time of every step; `compose.py` places the captions
by those markers and plays the wait between the swap and the claim faster.
A take fails if the testnet is slow or the faucet refuses: just run it again
with a fresh account.
