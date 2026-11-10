#!/usr/bin/env python3
"""The soundtrack of the presentation, synthesized from the same timeline the
picture is rendered from: a soft pad that follows a chord track, a quiet
plucked arpeggio, and sound design on the beats of each scene (ticks as
swaps are sealed, a bloom when the eclipse closes, a chime on the clearing
price). The narration is mixed in, and the music ducks under it.

Usage: music.py timeline.json voice_dir out.wav   (numpy and scipy required)
All times below are scene-relative seconds, or "l2+0.5" for half a second
into a scene's second narration line; the video starts `trim` seconds into
the timeline, so everything is shifted by that."""
import json, subprocess, sys, pathlib
import numpy as np
from scipy.signal import butter, lfilter, fftconvolve

SR = 44100
TL = json.load(open(sys.argv[1])); VOICE = pathlib.Path(sys.argv[2]); OUT = sys.argv[3]
TRIM = TL.get('trim', 0.0); DUR = TL['total'] - TRIM
N = int(DUR * SR) + SR
rng = np.random.default_rng(7)
scenes = {s['id']: s for s in TL['scenes']}

def at(scene, spec):
    """Video time of a scene-relative moment."""
    sc = scenes[scene]
    if isinstance(spec, str) and spec.startswith('l'):
        i, _, off = spec[1:].partition('+')
        return sc['t0'] + sc['lines'][int(i) - 1]['at'] + float(off or 0) - TRIM
    return sc['t0'] + float(spec) - TRIM

def midi(n): return 440.0 * 2 ** ((n - 69) / 12)

class Bus:
    def __init__(self): self.x = np.zeros((N, 2), dtype=np.float64)
    def add(self, start, sig, pan=0.0, amp=1.0):
        """Mixes a mono or stereo signal in at `start` seconds; pan is -1 (left) to 1 (right)."""
        i = int(round(start * SR))
        if i >= N or i + len(sig) <= 0: return
        if i < 0: sig = sig[-i:]; i = 0
        sig = sig[: N - i]
        if sig.ndim == 1:
            l, r = np.cos((pan + 1) * np.pi / 4), np.sin((pan + 1) * np.pi / 4)
            self.x[i:i + len(sig), 0] += sig * l * amp * 1.414
            self.x[i:i + len(sig), 1] += sig * r * amp * 1.414
        else:
            self.x[i:i + len(sig)] += sig * amp

def ramp(n, a, r):
    """An envelope of n samples with raised-cosine attack and release (seconds)."""
    e = np.ones(n); na, nr = min(n, int(a * SR)), min(n, int(r * SR))
    if na: e[:na] = 0.5 - 0.5 * np.cos(np.linspace(0, np.pi, na))
    if nr: e[-nr:] *= 0.5 + 0.5 * np.cos(np.linspace(0, np.pi, nr))
    return e

def pad_note(freq, dur, attack=1.6, release=2.6, bright=1.7):
    """A mellow sustained tone: a few harmonics, two slightly detuned voices left and right, a slow shimmer."""
    n = int((dur + release) * SR); t = np.arange(n) / SR
    out = np.zeros((n, 2))
    for ch, cents in ((0, -5.0), (1, 5.0)):
        f = freq * 2 ** (cents / 1200)
        s = np.zeros(n)
        for k in range(1, 9):
            if f * k > 6000: break
            s += np.sin(2 * np.pi * f * k * t + rng.uniform(0, 6.28)) / k ** bright
        lfo = 1 + 0.18 * np.sin(2 * np.pi * rng.uniform(0.07, 0.16) * t + rng.uniform(0, 6.28))
        out[:, ch] = s * lfo
    return out * ramp(n, attack, release)[:, None]

def pluck(freq, decay=0.3):
    n = int(decay * 7 * SR); t = np.arange(n) / SR
    s = np.sin(2 * np.pi * freq * t) + 0.32 * np.sin(4 * np.pi * freq * t) + 0.1 * np.sin(6 * np.pi * freq * t)
    return s * np.exp(-t / decay) * ramp(n, 0.004, 0.02)

def bell(freq, decay=1.1, index=2.4, ratio=3.5):
    n = int(decay * 5 * SR); t = np.arange(n) / SR
    s = np.sin(2 * np.pi * freq * t + index * np.exp(-t / 0.18) * np.sin(2 * np.pi * freq * ratio * t))
    return s * np.exp(-t / decay) * ramp(n, 0.003, 0.05)

def tick(freq=3200, decay=0.012):
    n = int(0.09 * SR); t = np.arange(n) / SR
    return (np.sin(2 * np.pi * freq * t) + 0.4 * rng.standard_normal(n)) * np.exp(-t / decay)

def thud(f0=42, sweep=58, decay=0.36):
    n = int(decay * 6 * SR); t = np.arange(n) / SR
    f = f0 + sweep * np.exp(-t / 0.05)
    return np.sin(2 * np.pi * np.cumsum(f) / SR) * np.exp(-t / decay) * ramp(n, 0.002, 0.05)

def whoosh(dur, lo, hi, peak=0.7):
    """Band-limited noise that swells to a peak (as a fraction of its length) and falls away."""
    n = int(dur * SR)
    b, a = butter(2, [lo / (SR / 2), hi / (SR / 2)], btype='band')
    s = lfilter(b, a, rng.standard_normal(n))
    x = np.linspace(0, 1, n)
    env = np.where(x < peak, (x / peak) ** 2.2, ((1 - x) / (1 - peak)) ** 1.6)
    return s * env / (np.abs(s).max() + 1e-9)

def swell(freqs, dur, peak=0.82):
    """A reverse-style tonal swell into a moment."""
    n = int(dur * SR); t = np.arange(n) / SR; x = np.linspace(0, 1, n)
    env = np.where(x < peak, (x / peak) ** 3, ((1 - x) / (1 - peak)) ** 2)
    return sum(np.sin(2 * np.pi * f * t + rng.uniform(0, 6.28)) for f in freqs) / len(freqs) * env

music, fx, wet = Bus(), Bus(), Bus()
MUSIC, REVERB = 0.145, 0.31   # bus gains: the bed sits well under the narration

# ---------------------------------------------------------------- the chord track
CH = {  # sub, pad voices (midi), arpeggio tones (midi)
    'Am': (33, [45, 52, 60, 64, 71], [57, 64, 69, 72, 76]),
    'F':  (29, [41, 48, 57, 64, 67], [53, 60, 65, 69, 72]),
    'C':  (36, [48, 55, 64, 67, 74], [60, 67, 72, 76, 79]),
    'G':  (31, [43, 50, 59, 62, 69], [55, 62, 67, 71, 74]),
    'Dm': (38, [50, 57, 65, 69, 76], [62, 69, 74, 77, 81]),
}
track = [
    ('problem', 0, 'Am'), ('title', 0, 'F'), ('title', 2.45, 'C'), ('title', 5.6, 'G'),
    ('shield', 0.3, 'Am'), ('shield', 'l2+0', 'F'),
    ('transfer', 0.3, 'C'), ('transfer', 'l2+0', 'G'),
    ('batch', 0.3, 'Am'), ('batch', 5.0, 'F'), ('batch', 'l2+0', 'Dm'), ('batch', 'l3+0', 'C'), ('batch', 'l3+3.6', 'G'),
    ('proofs', 0.3, 'Am'), ('proofs', 'l2+0', 'F'), ('proofs', 'l2+4.6', 'C'),
    ('intents', 0.3, 'Dm'), ('intents', 'l2+0', 'Am'), ('intents', 'l2+3.6', 'F'),
    ('product', 0.3, 'C'), ('product', 4.5, 'G'), ('product', 8.7, 'Am'), ('product', 12.9, 'F'),
    ('facts', 0.3, 'C'), ('facts', 4.7, 'G'), ('facts', 'l2+0', 'F'),
    ('outro', 0.3, 'C'),
]
chords = [(max(0.0, at(sc, sp)), name) for sc, sp, name in track]
for i, (t0, name) in enumerate(chords):
    t1 = chords[i + 1][0] if i + 1 < len(chords) else DUR + 0.5
    sub, voices, _ = CH[name]
    for j, m in enumerate(voices):
        music.add(t0 - 0.5, pad_note(midi(m), t1 - t0 + 0.5), amp=(0.2 if j < 3 else 0.11 if j == 3 else 0.06))
    n = int((t1 - t0 + 2.0) * SR); t = np.arange(n) / SR
    music.add(t0 - 0.3, np.sin(2 * np.pi * midi(sub) * t) * ramp(n, 1.2, 2.0), amp=0.12)
    music.add(t0 - 0.3, np.sin(2 * np.pi * midi(sub + 12) * t) * ramp(n, 1.2, 2.0), amp=0.06)

def chord_at(t):
    cur = chords[0][1]
    for t0, name in chords:
        if t0 <= t: cur = name
    return cur

# ---------------------------------------------------------------- a quiet arpeggio, from the shield scene to the last tile
BEAT = 60 / 84
arp_start, arp_end = at('shield', 0.6), at('facts', 'l2+2.2')
pattern = [0, 2, 3, 1, 4, 2, 3, 1]
k, t = 0, arp_start
while t < arp_end:
    tones = CH[chord_at(t + 0.02)][2]
    fade = min(1.0, (t - arp_start) / 5.0) * min(1.0, (arp_end - t) / 4.0)
    vel = (0.75 if k % 4 else 1.0) * (0.85 + 0.3 * rng.random())
    note = pluck(midi(tones[pattern[k % 8]]), decay=0.26)
    pan = 0.45 * (1 if k % 2 else -1)
    music.add(t, note, pan=pan, amp=0.05 * vel * fade)
    wet.add(t, note, pan=-pan, amp=0.045 * vel * fade)
    k += 1; t += BEAT / 2

# ---------------------------------------------------------------- sound design, scene by scene
def hit(scene, spec, sig, amp, pan=0.0, send=0.5):
    t = at(scene, spec)
    fx.add(t, sig, pan=pan, amp=amp)
    if send: wet.add(t, sig, pan=-pan, amp=amp * send)

# 1 · the problem: a tick as each of your rows lights up, a low weight under "Forever."
for i in range(6): hit('problem', f'l1+{1.6 + i * 0.22:.2f}', tick(2600 + 180 * i), 0.05, pan=0.5, send=0.3)
hit('problem', 'l1+4.7', bell(midi(64), decay=0.5, index=1.2), 0.05, pan=0.4)
hit('problem', 'l1+5.9', swell([midi(33), midi(45), midi(52)], 1.4), 0.16, send=0.3)
hit('problem', 'l1+7.1', thud(38, 40, 0.5), 0.3, send=0.15)
# 2 · the title: the moon slides in, the ring blooms
hit('title', 0.5, whoosh(2.0, 250, 2400, peak=0.9), 0.1, send=0.4)
hit('title', 1.0, swell([midi(48), midi(55), midi(64), midi(72)], 1.5, peak=0.94), 0.2, send=0.5)
hit('title', 2.42, thud(40, 50, 0.6), 0.34, send=0.2)
for m, a, p in ((72, 0.1, -0.3), (79, 0.08, 0.3), (84, 0.06, 0.0), (91, 0.035, 0.2)): hit('title', 2.45, bell(midi(m), decay=1.8), a, pan=p, send=0.9)
hit('title', 2.7, whoosh(1.4, 1500, 7000, peak=0.2), 0.035, send=0.6)
hit('title', scenes['title']['t1'] - scenes['title']['t0'] - 1.15, whoosh(1.0, 400, 3000, peak=0.6), 0.06, pan=-0.5)
# 3 · shield: the coin flies, the terminator rings, the chain strip types
hit('shield', 2.4, whoosh(1.1, 500, 3500, peak=0.8), 0.06, pan=-0.3)
for m, a in ((81, 0.08), (88, 0.055)): hit('shield', 3.35, bell(midi(m), decay=1.2), a, send=0.9)
hit('shield', 4.3, pluck(midi(57), 0.2), 0.07, pan=0.4)
hit('shield', 'l2+-0.35', whoosh(0.7, 300, 1800, peak=0.5), 0.04)
for i in range(5): hit('shield', f'l2+{0.9 + i * 0.07:.2f}', tick(3600, 0.008), 0.03, pan=-0.3, send=0.2)
for i in range(9): hit('shield', f'l2+{1.7 + i * 0.08:.2f}', tick(3300 + 60 * (i % 3), 0.008), 0.026, pan=0.0, send=0.2)
hit('shield', 'l2+2.7', whoosh(0.6, 180, 900, peak=0.3), 0.09, pan=0.3)
hit('shield', 'l2+3.1', bell(midi(69), decay=0.9, index=1.4), 0.05, pan=0.3)
# 4 · transfer: one proof, two notes out, and nothing else
hit('transfer', 1.3, pluck(midi(64), 0.22), 0.06, pan=-0.5)
hit('transfer', 1.5, bell(midi(72), decay=1.3), 0.08)
hit('transfer', 2.2, whoosh(0.5, 200, 1200, peak=0.3), 0.05, pan=-0.6)
hit('transfer', 2.95, pluck(midi(76), 0.25), 0.07, pan=0.6)
hit('transfer', 3.2, pluck(midi(72), 0.25), 0.07, pan=0.6)
hit('transfer', 'l2+-0.35', whoosh(0.7, 300, 1800, peak=0.5), 0.04)
for d in (0.75, 1.15, 1.85, 2.25):
    for i in range(4): hit('transfer', f'l2+{d + i * 0.06:.2f}', tick(3400, 0.008), 0.026, pan=-0.2 + 0.1 * i, send=0.2)
for i, d in enumerate((3.0, 3.2, 3.4)): hit('transfer', f'l2+{d}', pluck(midi(52 - 2 * i), 0.14), 0.07, pan=0.5)
# 5 · the batch: six seals, the epoch closes, the totals, one price, six claims
for i, d in enumerate((2.6, 3.5, 4.4, 5.4, 6.4, 7.4)):
    hit('batch', d, tick(2400, 0.02), 0.07, send=0.4)
    hit('batch', d, bell(midi(76 + [0, 3, 7, 10, 12, 15][i]), decay=0.6, index=1.6), 0.045, pan=(-0.5, 0.5)[i % 2], send=0.9)
hit('batch', 4.5, whoosh(0.7, 300, 1800, peak=0.5), 0.035)
hit('batch', 'l2+-1.3', swell([midi(38), midi(50), midi(57), midi(65)], 1.5, peak=0.93), 0.2, send=0.5)
hit('batch', 'l2+0.1', thud(40, 54, 0.6), 0.36, send=0.2)
hit('batch', 'l2+0.12', bell(midi(62), decay=1.6, index=3.0), 0.07, send=0.9)
hit('batch', 'l2+1.0', whoosh(0.8, 250, 1500, peak=0.75), 0.06)
hit('batch', 'l2+1.6', tick(1800, 0.03), 0.08, send=0.5)
hit('batch', 'l2+2.7', bell(midi(74), decay=1.0), 0.07, pan=-0.2, send=0.9)
hit('batch', 'l2+2.9', bell(midi(77), decay=1.0), 0.07, pan=0.2, send=0.9)
for m, a, p in ((72, 0.09, -0.3), (76, 0.08, 0.3), (79, 0.07, 0.0), (84, 0.05, 0.15)): hit('batch', 'l3+0.35', bell(midi(m), decay=1.6), a, pan=p, send=0.9)
hit('batch', 'l3+0.9', whoosh(0.8, 1200, 6000, peak=0.3), 0.035, send=0.6)
for i in range(6): hit('batch', f'l3+{2.9 + i * 0.12:.2f}', pluck(midi(72 + [0, 2, 4, 7, 9, 12][i]), 0.2), 0.06, pan=-0.6 + 0.24 * i, send=0.8)
for i in range(6): hit('batch', f'l3+{3.9 + i * 0.12:.2f}', tick(4200, 0.01), 0.03, pan=-0.6 + 0.24 * i, send=0.4)
# 6 · proofs: three cards, the prover runs, the stamp lands
for i, d in enumerate((1.0, 1.22, 1.44)): hit('proofs', d, pluck(midi(60 + 4 * i), 0.18), 0.06, pan=-0.5 + 0.5 * i)
n = int(1.9 * SR); tt = np.arange(n) / SR
riser = np.sin(2 * np.pi * np.cumsum(220 * 2 ** (tt / 1.9 * 1.0)) / SR) * ramp(n, 0.3, 0.25) * (0.3 + 0.7 * tt / 1.9)
hit('proofs', 'l2+0.5', riser, 0.03, send=0.6)
hit('proofs', 'l2+2.45', pluck(midi(81), 0.16), 0.07)
hit('proofs', 'l2+2.5', whoosh(0.9, 600, 4000, peak=0.6), 0.04, pan=0.4)
hit('proofs', 'l2+3.5', thud(60, 40, 0.16), 0.2, pan=0.5, send=0.1)
hit('proofs', 'l2+3.56', bell(midi(76), decay=0.9, index=1.2), 0.08, pan=0.5, send=0.8)
hit('proofs', 'l2+3.74', bell(midi(81), decay=1.2, index=1.2), 0.08, pan=0.5, send=0.8)
# 7 · intents: two bars, the match, the net leg goes out and a quote comes back
hit('intents', 1.2, whoosh(1.1, 300, 2500, peak=0.5), 0.045, pan=-0.4)
hit('intents', 2.0, whoosh(0.9, 300, 2500, peak=0.5), 0.04, pan=-0.4)
hit('intents', 3.5, tick(1500, 0.03), 0.08, pan=-0.3, send=0.5)
hit('intents', 3.52, pluck(midi(57), 0.2), 0.06, pan=-0.3)
hit('intents', 'l2+0.3', bell(midi(76), decay=1.0), 0.075, pan=0.1, send=0.9)
hit('intents', 'l2+2.9', whoosh(0.9, 700, 4500, peak=0.6), 0.04, pan=0.3)
hit('intents', 'l2+3.4', pluck(midi(69), 0.2), 0.07, pan=0.6)
hit('intents', 'l2+4.5', whoosh(0.9, 500, 3000, peak=0.5), 0.035, pan=0.5)
hit('intents', 'l2+5.0', bell(midi(79), decay=1.2), 0.07, pan=0.2, send=0.9)
hit('intents', 'l2+5.5', bell(midi(72), decay=1.4), 0.06, send=0.9)
# 8 · the wallet: the window rises, soft taps on each cut
hit('product', 0.4, whoosh(1.2, 250, 2200, peak=0.6), 0.05)
for i, d in enumerate((1.4, 5.1, 8.9, 11.2, 13.5)):
    hit('product', d, thud(70, 30, 0.1), 0.12, pan=0.3, send=0.1)
    hit('product', d, pluck(midi(64 + [0, 3, 5, 7, 10][i]), 0.2), 0.05, pan=0.3)
# 9 · what ships: a rising note per tile
for i, d in enumerate((1.0, 2.3, 3.6, 4.9, 6.4, 'l2+0.1')):
    hit('facts', d, pluck(midi(60 + [0, 4, 7, 12, 16, 19][i]), 0.24), 0.07, pan=-0.6 + 0.24 * i, send=0.8)
hit('facts', 'l2+0.1', bell(midi(84), decay=1.4), 0.05, send=0.9)
# 10 · outro: the ring returns
hit('outro', -0.9, swell([midi(48), midi(55), midi(64), midi(74)], 1.3, peak=0.92), 0.18, send=0.5)
hit('outro', 0.35, thud(40, 46, 0.6), 0.3, send=0.2)
for m, a, p in ((72, 0.1, -0.3), (79, 0.08, 0.3), (84, 0.06, 0.0), (88, 0.04, 0.2)): hit('outro', 0.4, bell(midi(m), decay=2.0), a, pan=p, send=0.9)
hit('outro', 3.4, pluck(midi(79), 0.25), 0.05, send=0.8)

# ---------------------------------------------------------------- reverb for the wet bus
def impulse(seed, length=2.8, tau=0.75):
    r = np.random.default_rng(seed); n = int(length * SR); t = np.arange(n) / SR
    ir = r.standard_normal(n) * np.exp(-t / tau)
    b, a = butter(2, 5200 / (SR / 2)); ir = lfilter(b, a, ir)
    pre = int(0.018 * SR); ir = np.concatenate([np.zeros(pre), ir])
    return ir / np.sqrt((ir ** 2).sum())
rev = np.stack([fftconvolve(wet.x[:, 0], impulse(11))[:N], fftconvolve(wet.x[:, 1], impulse(23))[:N]], axis=1)

# ---------------------------------------------------------------- the narration
def load(path):
    raw = subprocess.run(['ffmpeg', '-v', 'error', '-i', str(path), '-f', 'f32le', '-ac', '1', '-ar', str(SR), '-'], capture_output=True, check=True).stdout
    return np.frombuffer(raw, dtype=np.float32).astype(np.float64)
voice = np.zeros(N)
for v in TL['voice']:
    s = load(VOICE / f"{v['key']}.mp3"); i = int(round((v['start'] - TRIM) * SR))
    if i < 0: s = s[-i:]; i = 0
    voice[i:i + len(s)] += s[: N - i]
active = voice[np.abs(voice) > 0.01]
voice *= 0.1 / (np.sqrt((active ** 2).mean()) + 1e-9)          # narration at about -20 dBFS RMS while speaking

# the music ducks under the voice: a smoothed envelope of the narration drives its gain
hop = 441; frames = np.abs(voice[: (N // hop) * hop]).reshape(-1, hop).max(axis=1)
env = np.zeros_like(frames); e = 0.0
for i, x in enumerate(frames):
    e = x if x > e else e * 0.965                                 # instant attack, about 0.4 s release
    env[i] = e
env = np.interp(np.arange(N), np.arange(len(env)) * hop + hop / 2, np.clip(env / 0.05, 0, 1))
env = np.convolve(env, np.ones(2205) / 2205, mode='same')        # 50 ms of smoothing against zipper noise
duck_music = 1 - 0.56 * env                                        # about -7 dB while the voice speaks
duck_fx = 1 - 0.25 * env

# a little more music where there is no narration to carry the moment: the title and the outro
lift = np.ones(N); tt = np.arange(N) / SR
for a, b, g in ((at('title', 0.6), at('title', 2.6), 1.7), (at('outro', 0), DUR, 1.6)):
    lift += (g - 1) * np.clip((tt - a) / 1.2, 0, 1) * np.clip((b + 2.5 - tt) / 2.5, 0, 1)

bed = (music.x * MUSIC + rev * REVERB) * (duck_music * lift)[:, None]
sfx = fx.x * duck_fx[:, None]
mix = bed + sfx + voice[:, None]
fade = np.clip(tt / 0.25, 0, 1) * np.clip((DUR - tt) / 1.6, 0, 1)
mix = np.tanh(mix * fade[:, None] * 1.25) / 1.25
mix = mix[: int(DUR * SR)]

def db(x): return 20 * np.log10(np.sqrt((x ** 2).mean()) + 1e-12)
hb, ha = butter(2, 150 / (SR / 2), btype='high')
bed_hp = lfilter(hb, ha, bed[: len(mix), 0]); speaking = env[: len(mix)] > 0.5
print(f"bed above 150 Hz: {db(bed_hp[speaking]):.1f} dBFS under the voice, {db(bed_hp[~speaking]):.1f} dBFS in the gaps")
print(f"levels (dBFS RMS): voice {db(active * 0.1 / (np.sqrt((active ** 2).mean()) + 1e-9)):.1f}, bed {db(bed):.1f}, fx {db(sfx):.1f}, mix {db(mix):.1f}, peak {20 * np.log10(np.abs(mix).max()):.1f}")
pcm = (np.clip(mix, -1, 1) * 32767).astype('<i2')
subprocess.run(['ffmpeg', '-y', '-v', 'error', '-f', 's16le', '-ar', str(SR), '-ac', '2', '-i', '-', OUT], input=pcm.tobytes(), check=True)
print('wrote', OUT, f'{DUR:.2f} s')
