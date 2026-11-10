// Renders the deck frame by frame. Each worker is its own Chromium (GPU
// compositing where available), opens the deck, pauses its animations and,
// for every frame of its slice of the timeline, seeks to that frame's time
// and captures it; frames are piped straight into ffmpeg, so nothing but the
// encoded segments touches the disk. The result is exact and repeatable
// however slow the machine is.
//
//   node render.mjs --timeline t.json --video out.mp4 [--footage dir] [--workers 4] [--crf 16] [--captions 0] [--from s --to s]
//   node render.mjs --timeline t.json --stills 1.5,12,40 --out dir [--footage dir]
import { chromium } from 'playwright';
import { spawn, spawnSync } from 'node:child_process';
import { mkdirSync, readFileSync, existsSync, readdirSync, writeFileSync, rmSync } from 'node:fs';
import { fileURLToPath, pathToFileURL } from 'node:url';
import path from 'node:path';

const args = Object.fromEntries(process.argv.slice(2).reduce((a, x, i, all) => (x.startsWith('--') ? [...a, [x.slice(2), all[i + 1]]] : a), []));
const here = path.dirname(fileURLToPath(import.meta.url));
const TL = JSON.parse(readFileSync(args.timeline, 'utf8'));
const W = 1920, H = 1080, FPS = TL.fps;

let footage = null;
if (args.footage && existsSync(args.footage)) {
  const clips = {};
  for (const d of readdirSync(args.footage)) {
    try { const n = readdirSync(path.join(args.footage, d)).filter((f) => f.endsWith('.jpg')).length; if (n) clips[d] = n; } catch { /* not a directory */ }
  }
  footage = { base: pathToFileURL(args.footage).href.replace(/\/$/, ''), fps: FPS, clips };
}

const GPU = { channel: 'chromium', args: ['--enable-gpu', '--use-angle=metal', '--ignore-gpu-blocklist', '--force-color-profile=srgb', '--hide-scrollbars'] };
async function openDeck() {
  let browser;
  try { browser = await chromium.launch(GPU); } catch { browser = await chromium.launch({ args: ['--force-color-profile=srgb', '--hide-scrollbars'] }); }
  const ctx = await browser.newContext({ viewport: { width: W, height: H }, deviceScaleFactor: 1 });
  const page = await ctx.newPage();
  page.on('pageerror', (e) => console.log('pageerror', String(e).slice(0, 300)));
  page.on('console', (m) => { if (m.type() === 'error') console.log('console.error', m.text().slice(0, 300)); });
  await page.addInitScript(({ TL, footage, captions }) => { window.__TL = TL; window.__FOOTAGE = footage; window.__CAPTIONS = captions; }, { TL, footage, captions: args.captions !== '0' });
  await page.goto(pathToFileURL(path.join(here, 'deck.html')).href, { waitUntil: 'networkidle' });
  const info = await page.evaluate(() => window.__init());
  const cdp = await ctx.newCDPSession(page);
  const frame = async (t) => {
    await page.evaluate(async (t) => { window.__seek(t); await window.__settle(); }, t);
    const { data } = await cdp.send('Page.captureScreenshot', { format: 'png', optimizeForSpeed: true });
    return Buffer.from(data, 'base64');
  };
  return { browser, frame, info };
}

const started = Date.now();
if (args.stills) {
  mkdirSync(args.out, { recursive: true });
  const { browser, frame, info } = await openDeck();
  console.log('deck ready:', JSON.stringify(info));
  for (const t of args.stills.split(',').map(Number)) writeFileSync(path.join(args.out, `still-${t.toFixed(2).padStart(7, '0')}.png`), await frame(t));
  await browser.close();
  console.log(`stills rendered in ${((Date.now() - started) / 1000).toFixed(1)} s`);
} else {
  const first = Math.round(Number(args.from ?? TL.trim ?? 0) * FPS);
  const last = Math.round(Number(args.to ?? TL.total) * FPS);
  const workers = Math.max(1, Math.min(Number(args.workers ?? 4), last - first));
  const crf = String(args.crf ?? 16);
  const tmp = `${args.video}.parts`;
  rmSync(tmp, { recursive: true, force: true }); mkdirSync(tmp, { recursive: true });
  let done = 0;
  const per = Math.ceil((last - first) / workers);
  await Promise.all(Array.from({ length: workers }, async (_, k) => {
    const a = first + k * per, b = Math.min(last, a + per);
    if (a >= b) return;
    const { browser, frame, info } = await openDeck();
    if (k === 0) console.log('deck ready:', JSON.stringify(info), `· ${last - first} frames on ${workers} workers`);
    const seg = path.join(tmp, `seg-${String(k).padStart(2, '0')}.mp4`);
    const ff = spawn('ffmpeg', ['-y', '-loglevel', 'error', '-f', 'image2pipe', '-c:v', 'png', '-framerate', String(FPS), '-i', '-',
      '-vf', 'scale=out_color_matrix=bt709:out_range=tv,format=yuv420p', '-c:v', 'libx264', '-preset', 'slow', '-crf', crf, '-g', String(FPS * 2),
      '-colorspace', 'bt709', '-color_primaries', 'bt709', '-color_trc', 'bt709', '-color_range', 'tv', seg], { stdio: ['pipe', 'inherit', 'inherit'] });
    const closed = new Promise((res, rej) => ff.on('close', (c) => (c === 0 ? res() : rej(new Error(`ffmpeg exited ${c}`)))));
    for (let i = a; i < b; i++) {
      const png = await frame(i / FPS);
      if (!ff.stdin.write(png)) await new Promise((r) => ff.stdin.once('drain', r));
      done++;
      if (done % 240 === 0) console.log(`${done}/${last - first} frames · ${((Date.now() - started) / 1000).toFixed(0)} s`);
    }
    ff.stdin.end();
    await closed;
    await browser.close();
  }));
  const list = path.join(tmp, 'list.txt');
  writeFileSync(list, readdirSync(tmp).filter((f) => f.startsWith('seg-')).sort().map((f) => `file '${path.join(tmp, f)}'`).join('\n'));
  const r = spawnSync('ffmpeg', ['-y', '-loglevel', 'error', '-f', 'concat', '-safe', '0', '-i', list, '-c', 'copy', '-movflags', '+faststart', args.video], { stdio: 'inherit' });
  if (r.status !== 0) process.exit(1);
  rmSync(tmp, { recursive: true, force: true });
  console.log(`rendered ${done} frames in ${((Date.now() - started) / 1000).toFixed(1)} s → ${args.video}`);
}
