#!/usr/bin/env python3
"""Product cut: animated intro and outro cards with crossfades, the take with
gentle camera moves on key moments, captions that slide in, a neural
voice-over timed to the steps, and a quiet synthesized bed ducked under the
voice. H.264 1080p30, AAC."""
import json, math, subprocess, sys, pathlib

take, overlays, cards, voice, out = [pathlib.Path(a) for a in sys.argv[1:5]] + [sys.argv[5]]
def probe(path, what):
    return subprocess.run(['ffprobe', '-v', 'error', '-show_entries', what, '-of', 'csv=p=0', str(path)], capture_output=True, text=True).stdout.strip()

m = json.load(open(take / 'markers.json')); src = m['videoPath']; mk = m['markers']
W, H = (int(x) for x in probe(src, 'stream=width,height').split(',')[:2])
D = float(probe(src, 'format=duration') or 0) or mk['end'] + 0.5
durs = json.load(open(voice / 'durations.json'))
FPS, XF = 30, 0.8

# --- the take: the wait for the sealer plays faster
T1 = mk['geek'] + 4.0
T2 = max(T1 + 1.0, mk['claimed'] - 0.5)
F = max(2, min(8, math.ceil((T2 - T1) / 12)))
def mapt(t):
    if t <= T1: return t
    if t <= T2: return T1 + (t - T1) / F
    return t - (T2 - T1) * (1 - 1 / F)
Dm = mapt(D)
INTRO = min(7.0, max(5.5, durs['intro'] + 0.9))
OUTRO = min(6.5, max(5.0, durs['outro'] + 1.6))
main_start = INTRO - XF                      # where the take begins on the final timeline
outro_start = main_start + Dm - XF
total = outro_start + OUTRO

# --- captions by step, in mapped take time
windows = {
    'realms':   (0.5, mk['wrap_click'] + 3.2),
    'tokens':   (mk['wrap_click'] + 3.5, mk['shield_click'] - 0.3),
    'shield':   (mk['shield_click'], mk['swap_page'] - 0.3),
    'swap':     (mk['swap_page'], mk['swap_click'] + 0.6),
    'proof':    (mk['swap_click'] + 0.6, mk['geek'] - 0.3),
    'sealer':   (mk['geek'], mk['claimed'] - 0.3),
    'claim':    (mk['claimed'], mk['threshold_page'] - 0.3),
    'unshield': (mk['threshold_page'], mk['end'] - 0.2),
}
# --- camera moves: (start, end) in take time, zoom, focal point in source pixels
zooms = [
    (mk['shielded'] - 0.4, mk['shielded'] + 3.6, 1.22, (560, 300)),
    (mk['quoted'] - 0.6, mk['swap_click'] - 0.3, 1.18, (1120, 330)),
    (mk['claimed'] - 0.4, mk['claimed'] + 4.0, 1.20, (1120, 560)),
    (mk['unshielded'] - 0.4, mk['end'] - 0.3, 1.20, (560, 230)),
]
R = int(0.9 * FPS)
def ease(a, b):  # 0..1 inside [a,b] frames with cosine ramps of R frames
    return f"((1-cos(PI*clip(min(in-{a},{b}-in)/{R},0,1)))/2)"
zexpr, xexpr, yexpr = '1', '0', '0'
for (a, b, z, (fx, fy)) in reversed(zooms):
    A, B = int(mapt(a) * FPS), int(mapt(b) * FPS)
    e = ease(A, B)
    zexpr = f"if(between(in,{A},{B}),1+{z - 1:.3f}*{e},{zexpr})"
    xexpr = f"if(between(in,{A},{B}),clip({fx}-iw/zoom/2,0,iw-iw/zoom),{xexpr})"
    yexpr = f"if(between(in,{A},{B}),clip({fy}-ih/zoom/2,0,ih-ih/zoom),{yexpr})"

inputs = ['-i', src, '-i', str(cards / 'intro.webm'), '-i', str(cards / 'outro.webm')]
caps = []
for name, (a, b) in windows.items():
    a, b = mapt(a), mapt(b)
    if b - a < 1.2: continue
    inputs += ['-framerate', str(FPS), '-loop', '1', '-t', f'{b - a:.3f}', '-i', str(overlays / f'cap-{name}.png')]
    caps.append((3 + len(caps), a, b))
# --- voice lines on the final timeline, never overlapping
plan = {'intro': 0.7}
for name, (a, _) in windows.items():
    plan[name] = main_start + mapt(a) + 0.2
plan['outro'] = outro_start + XF + 0.3
order = ['intro', 'realms', 'tokens', 'shield', 'swap', 'proof', 'sealer', 'claim', 'unshield', 'outro']
starts, prev_end = {}, 0.0
for name in order:
    s = max(plan[name], prev_end + 0.45)
    starts[name] = s
    prev_end = s + durs[name]
voice_idx = {}
for name in order:
    voice_idx[name] = len(inputs) // 2 if False else None
vi = 3 + len(caps)
for name in order:
    inputs += ['-i', str(voice / f'{name}.mp3')]
    voice_idx[name] = vi; vi += 1

f = []
f.append(f"[0:v]trim=0:{T1:.3f},setpts=PTS-STARTPTS[a]")
f.append(f"[0:v]trim={T1:.3f}:{T2:.3f},setpts=(PTS-STARTPTS)/{F}[b]")
f.append(f"[0:v]trim={T2:.3f},setpts=PTS-STARTPTS[c]")
f.append(f"[a][b][c]concat=n=3:v=1:a=0,fps={FPS},zoompan=z='{zexpr}':x='{xexpr}':y='{yexpr}':d=1:s={W}x{H}:fps={FPS}[m0]")
prev = 'm0'
for i, (idx, a, b) in enumerate(caps):
    dur = b - a
    f.append(f"[{idx}:v]format=rgba,fade=t=in:st=0:d=0.35:alpha=1,fade=t=out:st={dur - 0.35:.3f}:d=0.35:alpha=1,setpts=PTS-STARTPTS+{a:.3f}/TB[c{i}]")
    f.append(f"[{prev}][c{i}]overlay=x=(W-w)/2:y='H-h-56+22*(1-min(1,(t-{a:.3f})/0.4))':eval=frame:enable='between(t,{a:.3f},{b:.3f})'[m{i + 1}]")
    prev = f'm{i + 1}'
f.append(f"[1:v]trim=0:{INTRO:.3f},setpts=PTS-STARTPTS,fps={FPS},format=yuv420p,fade=t=in:st=0:d=0.6[intro]")
f.append(f"[2:v]trim=0:{OUTRO:.3f},setpts=PTS-STARTPTS,fps={FPS},format=yuv420p,fade=t=out:st={OUTRO - 0.8:.3f}:d=0.8[outro]")
f.append(f"[{prev}]format=yuv420p[mv]")
f.append(f"[intro][mv]xfade=transition=fade:duration={XF}:offset={INTRO - XF:.3f}[im]")
f.append(f"[im][outro]xfade=transition=fade:duration={XF}:offset={outro_start:.3f},format=yuv420p[v]")
# --- audio: voice lines delayed into place, a soft synthesized bed ducked under them
vparts = []
for name in order:
    f.append(f"[{voice_idx[name]}:a]aformat=sample_rates=44100:channel_layouts=stereo,adelay={int(starts[name] * 1000)}|{int(starts[name] * 1000)},volume=1.0[v_{name}]")
    vparts.append(f"[v_{name}]")
f.append(f"{''.join(vparts)}amix=inputs={len(vparts)}:normalize=0,asplit=2[voice][vsc]")
f.append(f"sine=frequency=110:sample_rate=44100:duration={total:.2f}[s1];sine=frequency=164.8:sample_rate=44100:duration={total:.2f}[s2];sine=frequency=220:sample_rate=44100:duration={total:.2f}[s3];sine=frequency=329.6:sample_rate=44100:duration={total:.2f}[s4]")
f.append("[s1][s2][s3][s4]amix=inputs=4:normalize=0,tremolo=f=0.11:d=0.5,lowpass=f=700,aecho=0.7:0.6:900|1700:0.25|0.15,aformat=channel_layouts=stereo,volume=0.045,afade=t=in:st=0:d=3," + f"afade=t=out:st={total - 4:.2f}:d=4[bed0]")
f.append("[bed0][vsc]sidechaincompress=threshold=0.02:ratio=6:attack=40:release=600:makeup=1[bed]")
f.append("[voice][bed]amix=inputs=2:normalize=0,loudnorm=I=-16:TP=-1.5:LRA=11,aresample=44100[aout]")
cmd = ['ffmpeg', '-y', '-loglevel', 'error', *inputs, '-filter_complex', ';'.join(f), '-map', '[v]', '-map', '[aout]', '-t', f'{total:.2f}',
       '-c:v', 'libx264', '-preset', 'slow', '-crf', '18', '-pix_fmt', 'yuv420p', '-r', str(FPS), '-c:a', 'aac', '-b:a', '160k', '-movflags', '+faststart', out]
print(f"take {W}x{H} {D:.1f}s, wait {T1:.1f}..{T2:.1f}s x{F}, main {Dm:.1f}s; intro {INTRO:.1f}s outro {OUTRO:.1f}s; total {total:.1f}s; captions {len(caps)}")
print('voice at:', {k: round(v, 1) for k, v in starts.items()})
subprocess.run(cmd, check=True)
print(probe(out, 'format=duration'), 'seconds written')
