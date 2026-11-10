#!/usr/bin/env python3
"""Lays the scenes out in time from the measured narration: each scene lasts
lead + its lines (with gaps) + tail, never less than its minimum, and
overlaps the next one slightly so transitions never show an empty frame.
Also cuts the narration into caption cues that follow the spoken words, and
writes them as an .srt next to the timeline.
Usage: timeline.py story.json voice_dir timeline.json"""
import json, re, sys, pathlib
story = json.load(open(sys.argv[1])); vdir = pathlib.Path(sys.argv[2])
durs = json.load(open(vdir / 'durations.json')); words = json.load(open(vdir / 'words.json'))
fps, overlap, gap = story['fps'], story['overlap'], story['gap']

def cues(line, w, t_line, dur):
    """Caption cues for one line: display tokens grouped into short phrases, timed by the words they cover."""
    toks = []  # (shown text, number of spoken words it stands for)
    for m in re.finditer(r'\{([^|}]*)\|([^}]*)\}(\S*)|(\S+)', line):
        if m.group(4): toks.append((m.group(4), 1))
        else: toks.append((m.group(1) + m.group(3), len(m.group(2).split())))
    n_spoken = sum(k for _, k in toks)
    starts = []
    if n_spoken == len(w):
        i = 0
        for _, k in toks: starts.append(w[i]['t']); i += k
    else:  # the voice split the words differently: spread the tokens evenly by length instead
        total = sum(len(s) + 1 for s, _ in toks); acc = 0
        for s, _ in toks: starts.append(dur * acc / total); acc += len(s) + 1
    # clauses end at punctuation; a cue is one clause, or a few short ones, and never runs past a sentence
    clauses, cur = [], []
    for i, (s, _) in enumerate(toks):
        cur.append(i)
        if s[-1] in '.!?,:;': clauses.append(cur); cur = []
    if cur: clauses.append(cur)
    text = lambda idx: ' '.join(toks[i][0] for i in idx)
    groups, cur = [], []
    for c in clauses:
        if cur and (len(text(cur)) + 1 + len(text(c)) > 46 or toks[cur[-1]][0][-1] in '.!?'): groups.append(cur); cur = []
        cur = cur + c
    if cur: groups.append(cur)
    out = []
    for g in groups:
        if len(text(g)) > 52:  # one long clause: break it at the word nearest its middle
            half = len(text(g)) / 2; k = min(range(1, len(g)), key=lambda j: abs(len(text(g[:j])) - half))
            out += [[starts[g[0]], text(g[:k])], [starts[g[k]], text(g[k:])]]
        else: out.append([starts[g[0]], text(g)])
    res = []
    for i, (t, text) in enumerate(out):
        end = out[i + 1][0] - 0.06 if i + 1 < len(out) else dur + 0.25
        res.append({'start': round(t_line + max(0, t - 0.06), 3), 'end': round(t_line + end, 3), 'text': text.rstrip(',;:')})
    return res

t, scenes, voice, captions = 0.0, [], [], []
for n, sc in enumerate(story['scenes']):
    t0 = t; cur = sc['lead']; lines = []
    for i, line in enumerate(sc['lines']):
        key = f"{sc['id']}-{i + 1}"; d = durs[key]
        lines.append({'key': key, 'at': round(cur, 3), 'dur': d})
        voice.append({'key': key, 'start': round(t0 + cur, 3), 'dur': d})
        captions += cues(line, words[key], t0 + cur, d)
        cur += d + gap
    length = max(sc.get('min', 0), cur - gap + sc['tail'])
    length = round(length * fps) / fps
    scenes.append({'id': sc['id'], 'n': n + 1, 'chapter': sc['chapter'], 't0': round(t0, 3), 't1': round(t0 + length, 3), 'lines': lines})
    t = t0 + length - overlap
total = scenes[-1]['t1']
trim = story.get('trim', 0.0)  # the video starts this far into the timeline, on a frame that is already composed
json.dump({'fps': fps, 'total': round(total, 3), 'frames': int(round(total * fps)), 'trim': trim, 'scenes': scenes, 'voice': voice, 'captions': captions}, open(sys.argv[3], 'w'), indent=1)

def stamp(x):
    x = max(0.0, x - trim); h, r = divmod(x, 3600); m, s = divmod(r, 60)
    return f'{int(h):02d}:{int(m):02d}:{int(s):02d},{int(round((s - int(s)) * 1000)):03d}'
srt = pathlib.Path(sys.argv[3]).with_suffix('.srt')
srt.write_text('\n'.join(f"{i + 1}\n{stamp(c['start'])} --> {stamp(c['end'])}\n{c['text']}\n" for i, c in enumerate(captions)))
for s in scenes:
    print(f"{s['n']:2d} {s['id']:9s} {s['t0']:7.2f} → {s['t1']:7.2f}  ({s['t1'] - s['t0']:5.2f}s)  lines at " + ', '.join(f"{l['at']:.2f}" for l in s['lines']))
print(len(captions), 'caption cues →', srt.name)
print('timeline', round(total, 2), 's; video', round(total - trim, 2), 's,', int(round((total - trim) * fps)), 'frames')
