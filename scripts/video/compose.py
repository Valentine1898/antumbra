#!/usr/bin/env python3
"""Composes the product video: intro card, the recording with step captions and
a sped-up sealer wait, outro card; H.264 1080p30 with a silent audio track."""
import json, math, subprocess, sys, pathlib

video_dir, overlays, out = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2]), sys.argv[3]
m = json.load(open(video_dir / 'markers.json'))
src = m['videoPath']; mk = m['markers']
probe = subprocess.run(['ffprobe', '-v', 'error', '-select_streams', 'v:0', '-show_entries', 'stream=width,height', '-of', 'csv=p=0', src], capture_output=True, text=True).stdout.strip().split(',')
W, H = int(probe[0]), int(probe[1])
D = float(subprocess.run(['ffprobe', '-v', 'error', '-show_entries', 'format=duration', '-of', 'csv=p=0', src], capture_output=True, text=True).stdout.strip() or mk['end'] + 0.5)
if D != D or D <= 0:  # NaN guard
    D = mk['end'] + 0.5

# The wait for the sealer plays faster; everything else at real speed.
T1 = mk['geek'] + 4.0
T2 = max(T1 + 1.0, mk['claimed'] - 0.5)
F = max(2, min(8, math.ceil((T2 - T1) / 12)))
def mapt(t):
    if t <= T1: return t
    if t <= T2: return T1 + (t - T1) / F
    return t - (T2 - T1) * (1 - 1 / F)
Dm = mapt(D)

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
INTRO, OUTRO, FADE = 3.6, 4.6, 0.6
inputs = ['-i', src, '-framerate', '30', '-loop', '1', '-t', str(INTRO), '-i', str(overlays / 'intro.png'),
          '-framerate', '30', '-loop', '1', '-t', str(OUTRO), '-i', str(overlays / 'outro.png')]
caps = []
for name, (a, b) in windows.items():
    a, b = mapt(a), mapt(b)
    if b - a < 1.2: continue
    idx = 3 + len(caps)
    inputs += ['-framerate', '30', '-loop', '1', '-t', f'{b - a:.3f}', '-i', str(overlays / f'cap-{name}.png')]
    caps.append((idx, a, b))
audio_idx = 3 + len(caps)
inputs += ['-f', 'lavfi', '-i', 'anullsrc=r=44100:cl=stereo']

f = []
f.append(f"[0:v]trim=0:{T1:.3f},setpts=PTS-STARTPTS[a]")
f.append(f"[0:v]trim={T1:.3f}:{T2:.3f},setpts=(PTS-STARTPTS)/{F}[b]")
f.append(f"[0:v]trim={T2:.3f},setpts=PTS-STARTPTS[c]")
f.append(f"[a][b][c]concat=n=3:v=1:a=0,fps=30,fade=t=in:st=0:d={FADE},fade=t=out:st={Dm - FADE:.3f}:d={FADE}[m0]")
prev = 'm0'
for i, (idx, a, b) in enumerate(caps):
    dur = b - a
    f.append(f"[{idx}:v]format=rgba,fade=t=in:st=0:d=0.35:alpha=1,fade=t=out:st={dur - 0.35:.3f}:d=0.35:alpha=1,setpts=PTS-STARTPTS+{a:.3f}/TB[c{i}]")
    f.append(f"[{prev}][c{i}]overlay=x=(W-w)/2:y=H-h-56:enable='between(t,{a:.3f},{b:.3f})'[m{i + 1}]")
    prev = f'm{i + 1}'
f.append(f"[1:v]format=yuv420p,fade=t=in:st=0:d={FADE},fade=t=out:st={INTRO - FADE:.3f}:d={FADE},setpts=PTS-STARTPTS[intro]")
f.append(f"[2:v]format=yuv420p,fade=t=in:st=0:d={FADE},fade=t=out:st={OUTRO - FADE:.3f}:d={FADE},setpts=PTS-STARTPTS[outro]")
f.append(f"[intro][{prev}][outro]concat=n=3:v=1:a=0,format=yuv420p[v]")
cmd = ['ffmpeg', '-y', '-loglevel', 'error', *inputs, '-filter_complex', ';'.join(f), '-map', '[v]', '-map', f'{audio_idx}:a', '-shortest',
       '-c:v', 'libx264', '-preset', 'slow', '-crf', '19', '-pix_fmt', 'yuv420p', '-r', '30', '-c:a', 'aac', '-b:a', '64k', '-movflags', '+faststart', out]
print(f"source {W}x{H} {D:.1f}s; wait {T1:.1f}..{T2:.1f}s x{F}; main {Dm:.1f}s; total ~{INTRO + Dm + OUTRO:.1f}s; captions {len(caps)}")
subprocess.run(cmd, check=True)
print(subprocess.run(['ffprobe', '-v', 'error', '-show_entries', 'format=duration:stream=width,height', '-of', 'default=nw=1', out], capture_output=True, text=True).stdout)
