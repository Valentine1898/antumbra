// Drives the deck from one number: the time of the frame being rendered.
// CSS animations are paused and seeked; everything CSS cannot express
// (counters, typing, the footage, the lights, the grain) is set per frame.
(() => {
  const TL = window.__TL;
  const FOOT = window.__FOOTAGE || null;
  const $ = (s, r = document) => r.querySelector(s);
  const $$ = (s, r = document) => [...r.querySelectorAll(s)];
  const clamp = (x, a = 0, b = 1) => Math.min(b, Math.max(a, x));
  const sm = (a, b, x) => { const t = clamp((x - a) / (b - a)); return t * t * (3 - 2 * t); };
  const easeOut = (t) => 1 - Math.pow(1 - clamp(t), 3);
  const lerp = (a, b, t) => a + (b - a) * t;
  const fmt = (v, dec) => v.toLocaleString('en-US', { minimumFractionDigits: dec, maximumFractionDigits: dec });

  // ---- scenes: starts, ends and narration line offsets become CSS variables
  const scenes = {};
  for (const sc of TL.scenes) {
    const el = document.getElementById(sc.id);
    scenes[sc.id] = { ...sc, el };
    el.style.setProperty('--t0', `${sc.t0}s`);
    el.style.setProperty('--t1', `${sc.t1}s`);
    el.style.setProperty('--len', `${(sc.t1 - sc.t0).toFixed(3)}s`);
    sc.lines.forEach((l, i) => el.style.setProperty(`--l${i + 1}`, `${l.at}s`));
    const ch = $('.chapter', el);
    if (ch && sc.chapter) ch.innerHTML = `<b>${String(sc.n).padStart(2, '0')}</b><span>${sc.chapter}</span>`;
  }
  const sceneOf = (el) => scenes[el.closest('.scene').id];
  /// "1.2" is seconds after the scene starts; "l2+0.5" is half a second into its second narration line.
  const when = (el, raw = el.dataset.d || '0') => {
    const sc = sceneOf(el);
    const m = String(raw).match(/^l(\d)([+-][\d.]+)?$/);
    return sc.t0 + (m ? sc.lines[+m[1] - 1].at + parseFloat(m[2] || 0) : parseFloat(raw));
  };

  // ---- generated pieces
  const ticks = $('#ticks');
  for (let i = 0; i < 20; i++) {
    const a = (i / 20) * Math.PI * 2 - Math.PI / 2;
    const l = document.createElementNS('http://www.w3.org/2000/svg', 'line');
    l.setAttribute('class', 'tick');
    l.setAttribute('x1', 220 + Math.cos(a) * 172); l.setAttribute('y1', 220 + Math.sin(a) * 172);
    l.setAttribute('x2', 220 + Math.cos(a) * 184); l.setAttribute('y2', 220 + Math.sin(a) * 184);
    ticks.appendChild(l);
  }
  const segs = $('#segs');
  const segFills = TL.scenes.map((sc) => {
    const i = document.createElement('i'); i.style.flexGrow = String(sc.t1 - sc.t0);
    const b = document.createElement('b'); i.appendChild(b); segs.appendChild(i);
    return { b, sc };
  });

  // ---- per-frame pieces
  const counters = $$('[data-count]').map((el) => ({
    el, t: when(el), dur: +(el.dataset.dur || 1.2), from: +(el.dataset.from || 0), to: +el.dataset.to,
    dec: +(el.dataset.dec || 0), pre: el.dataset.pre, trim: el.dataset.trim === '1', last: null,
  }));
  const steppers = $$('[data-steps]').map((el) => ({ el, at: el.dataset.steps.split(',').map((s) => when(el, s.trim())), last: null }));
  const typers = $$('[data-type]').map((el) => {
    const text = el.textContent;
    el.innerHTML = '<span></span><span style="opacity:0"></span>';
    return { el, on: el.firstChild, off: el.lastChild, text, t: when(el), cps: +(el.dataset.cps || 46), last: -1 };
  });

  // The lights: where the three blobs sit for each scene (centre x, y, scale, opacity).
  const P = (b1, b2, b3) => [b1, b2, b3];
  const moods = {
    problem: P([200, 80, 1.0, 0.3], [1650, 300, 1.15, 0.6], [1000, 1250, 1.0, 0.22]),
    title: P([360, 160, 1.0, 0.5], [1560, 380, 1.0, 0.5], [960, 1320, 1.0, 0.28]),
    shield: P([260, 430, 1.25, 0.6], [1900, 940, 0.9, 0.22], [700, 1290, 0.9, 0.2]),
    transfer: P([100, 1040, 0.8, 0.16], [1520, 140, 1.0, 0.34], [960, 1420, 0.8, 0.12]),
    batch: P([960, 520, 0.7, 0.2], [1720, 180, 1.0, 0.4], [200, 1240, 0.9, 0.14]),
    batchClear: P([960, 500, 0.95, 0.42], [1720, 180, 1.0, 0.34], [200, 1240, 0.9, 0.16]),
    proofs: P([1720, 920, 0.9, 0.28], [240, 240, 1.1, 0.5], [960, 1380, 0.9, 0.16]),
    intents: P([1500, 400, 1.0, 0.42], [300, 820, 1.0, 0.36], [960, 1380, 0.9, 0.14]),
    product: P([420, 200, 1.0, 0.45], [1500, 620, 1.2, 0.55], [900, 1320, 1.0, 0.2]),
    facts: P([200, 920, 0.9, 0.3], [1700, 250, 1.0, 0.42], [960, 1420, 0.9, 0.14]),
    outro: P([360, 160, 1.0, 0.5], [1560, 380, 1.0, 0.5], [960, 1320, 1.0, 0.28]),
  };
  const moodTrack = TL.scenes.map((sc) => ({ t: sc.t0, m: moods[sc.id] }));
  moodTrack.push({ t: scenes.batch.t0 + scenes.batch.lines[1].at, m: moods.batchClear });
  moodTrack.sort((a, b) => a.t - b.t);
  const blobs = $$('.blob');
  const grain = $('.grain');
  const curtain = $('#curtain');
  const cap = $('#cap'), capText = $('#cap span');
  const cues = window.__CAPTIONS === false ? [] : TL.captions || [];
  let capLast = '';
  const brand = $('#brand'), brandText = $('#brand .t');

  // The wallet footage: which clip plays when, relative to the product scene.
  const shot = $('#shot'), cut = $('#cut');
  const P0 = scenes.product.t0;
  const clips = [
    { id: 'a', on: 1.4, off: 5.1 }, { id: 'b', on: 5.1, off: 8.9 }, { id: 'c1', on: 8.9, off: 11.2 },
    { id: 'c2', on: 11.2, off: 13.5 }, { id: 'd', on: 13.5, off: 16.75 },
  ];
  let shotSrc = '', shotPending = null;
  window.__CLIPS = clips;

  let anims = [];
  window.__init = async () => {
    await document.fonts.ready;
    await Promise.all(['600 96px Inter', '500 20px Inter', '400 28px Inter', 'italic 400 54px "Instrument Serif"', '500 20px "JetBrains Mono"', '400 20px "JetBrains Mono"'].map((f) => document.fonts.load(f)));
    anims = document.getAnimations();
    for (const a of anims) a.pause();
    return { total: TL.total, fps: TL.fps, animations: anims.length };
  };

  window.__seek = (t) => {
    for (const a of anims) a.currentTime = t * 1000;
    for (const sc of TL.scenes) scenes[sc.id].el.style.visibility = t >= sc.t0 - 0.02 && t <= sc.t1 + 0.02 ? 'visible' : 'hidden';

    for (const c of counters) {
      let txt;
      if (t < c.t) txt = c.pre ?? fmt(c.from, c.dec);
      else {
        const v = lerp(c.from, c.to, easeOut((t - c.t) / c.dur));
        txt = c.trim && Number.isInteger(+v.toFixed(c.dec)) ? fmt(v, 0) : fmt(v, c.dec);
      }
      if (txt !== c.last) { c.el.textContent = txt; c.last = txt; }
    }
    for (const s of steppers) {
      const n = s.at.filter((x) => x <= t).length;
      if (n !== s.last) { s.el.textContent = String(n); s.last = n; }
    }
    for (const y of typers) {
      const n = clamp(Math.floor((t - y.t) * y.cps), 0, y.text.length);
      if (n !== y.last) {
        y.on.textContent = y.text.slice(0, n); y.off.textContent = y.text.slice(n);
        y.on.classList.toggle('caret', n > 0 && n < y.text.length);
        y.last = n;
      }
    }

    // lights
    let cur = moodTrack[0].m;
    for (let i = 1; i < moodTrack.length; i++) {
      const w = sm(moodTrack[i].t - 0.4, moodTrack[i].t + 1.6, t);
      if (w > 0) cur = cur.map((b, k) => b.map((v, j) => lerp(v, moodTrack[i].m[k][j], w)));
    }
    blobs.forEach((b, k) => {
      const [x, y, s, o] = cur[k];
      const dx = 46 * Math.sin(t * 0.21 + k * 2.1), dy = 34 * Math.cos(t * 0.17 + k * 1.3);
      b.style.transform = `translate(${(x + dx).toFixed(1)}px, ${(y + dy).toFixed(1)}px) scale(${s.toFixed(3)})`;
      b.style.opacity = o.toFixed(3);
    });
    const f = Math.round(t * TL.fps);
    grain.style.transform = `translate(${(f * 73) % 220}px, ${(f * 151) % 220}px)`;

    // chrome
    const tEnd = scenes.title.t1, oStart = scenes.outro.t0;
    brand.style.opacity = (sm(tEnd - 0.22, tEnd - 0.04, t) * (1 - sm(oStart - 0.1, oStart + 0.5, t))).toFixed(3);
    brandText.style.opacity = sm(tEnd - 0.05, tEnd + 0.5, t).toFixed(3);
    brandText.style.transform = `translateX(${((1 - sm(tEnd - 0.05, tEnd + 0.5, t)) * -14).toFixed(1)}px)`;
    segs.style.opacity = (sm(0.4, 1.4, t) * (1 - sm(TL.total - 1.6, TL.total - 0.8, t))).toFixed(3);
    for (const { b, sc } of segFills) b.style.transform = `scaleX(${clamp((t - sc.t0) / (sc.t1 - sc.t0)).toFixed(4)})`;
    curtain.style.opacity = Math.max(1 - sm(0, 0.7, t), sm(TL.total - 1.1, TL.total - 0.15, t)).toFixed(3);

    // captions
    const cue = cues.find((c) => t >= c.start && t < c.end);
    if (cue) {
      if (cue.text !== capLast) { capText.textContent = cue.text; capLast = cue.text; }
      cap.style.opacity = (sm(cue.start, cue.start + 0.14, t) * (1 - sm(cue.end - 0.1, cue.end, t))).toFixed(3);
      capText.style.transform = `translateY(${((1 - sm(cue.start, cue.start + 0.22, t)) * 8).toFixed(1)}px)`;
    } else cap.style.opacity = '0';

    // footage
    if (FOOT && t >= P0 - 0.5 && t <= scenes.product.t1 + 0.1) {
      const rel = t - P0;
      const clip = clips.find((c) => rel < c.off) || clips[clips.length - 1];
      const n = FOOT.clips[clip.id];
      const idx = clamp(Math.floor((rel - clip.on) * FOOT.fps), 0, n - 1);
      const src = `${FOOT.base}/${clip.id}/${String(idx + 1).padStart(4, '0')}.jpg`;
      if (src !== shotSrc) {
        shotSrc = src;
        shot.src = src;
        shotPending = shot.decode().catch(() => undefined);
      }
      let o = 0;
      for (const c of clips.slice(1)) o = Math.max(o, 1 - sm(0, 0.16, Math.abs(rel - c.on)));
      cut.style.opacity = (o * 0.85).toFixed(3);
    }
  };

  /// Resolves when everything the last seek asked for (the footage frame) is decoded.
  window.__settle = async () => { if (shotPending) { await shotPending; shotPending = null; } };
})();
