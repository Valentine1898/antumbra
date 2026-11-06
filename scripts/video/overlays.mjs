// Renders the intro card, the outro card and the step captions as PNGs in the
// app's own design language (same fonts, blobs, grain), for ffmpeg to overlay.
import { chromium } from 'playwright';
import { mkdirSync, writeFileSync } from 'node:fs';

const OUT = process.env.OUT;
const W = Number(process.env.W ?? 1920), H = Number(process.env.H ?? 1080);
mkdirSync(OUT, { recursive: true });

const base = `
<link href="https://fonts.googleapis.com/css2?family=Inter:wght@400;500;600;700&family=Instrument+Serif:ital@0;1&display=swap" rel="stylesheet">
<style>
  :root { --bg:#06070a; --ink:#f1f2f5; --muted:#8d93a3; --orange:#ff6a3d; --blue:#4f7dff; --cream:#f6efe6; }
  * { box-sizing: border-box; }
  html, body { margin: 0; background: transparent; }
  body { font: 15px/1.5 Inter, system-ui, sans-serif; color: var(--ink); -webkit-font-smoothing: antialiased; }
  .card { position: relative; width: ${W}px; height: ${H}px; overflow: hidden; background: var(--bg); }
  .blob { position: absolute; width: 70vmax; height: 70vmax; border-radius: 50%; filter: blur(80px); opacity: .5; mix-blend-mode: screen; }
  .b1 { left: -20vmax; top: -25vmax; background: radial-gradient(circle at 40% 40%, var(--orange) 0%, rgba(255,106,61,.35) 30%, transparent 62%); }
  .b2 { right: -25vmax; top: -10vmax; background: radial-gradient(circle at 50% 50%, var(--blue) 0%, rgba(79,125,255,.35) 32%, transparent 64%); }
  .b3 { left: 20vmax; bottom: -35vmax; width: 60vmax; height: 60vmax; opacity: .28; background: radial-gradient(circle at 50% 50%, var(--cream) 0%, rgba(246,239,230,.25) 28%, transparent 60%); }
  .disc { position: absolute; left: 50%; top: 50%; width: 62vmin; height: 62vmin; margin: -31vmin 0 0 -31vmin; border-radius: 50%; background: radial-gradient(circle at 50% 50%, #05060a 0 58%, rgba(5,6,10,.75) 62%, transparent 72%); }
  .grain { position: absolute; inset: 0; opacity: .16; mix-blend-mode: overlay; background-image: url("data:image/svg+xml;utf8,<svg xmlns='http://www.w3.org/2000/svg' width='220' height='220'><filter id='n'><feTurbulence type='fractalNoise' baseFrequency='0.9' numOctaves='2' stitchTiles='stitch'/><feColorMatrix values='0 0 0 0 1 0 0 0 0 1 0 0 0 0 1 0 0 0 0.6 0'/></filter><rect width='100%' height='100%' filter='url(%23n)'/></svg>"); }
  .center { position: absolute; inset: 0; display: grid; place-items: center; text-align: center; }
  .eclipse { width: 54px; height: 54px; border-radius: 50%; display: inline-block; margin-bottom: 26px;
    background: radial-gradient(circle at 50% 50%, #06070a 0 54%, transparent 57%), conic-gradient(from 210deg, var(--orange), var(--blue), var(--cream), var(--orange));
    box-shadow: 0 0 40px rgba(255,106,61,.5), 0 0 90px rgba(79,125,255,.3); }
  h1 { margin: 0; font-size: 112px; font-weight: 600; letter-spacing: -0.04em; line-height: 1; }
  .tag { margin: 22px 0 0; font-family: "Instrument Serif", Georgia, serif; font-style: italic; font-size: 44px; color: var(--cream); opacity: .95; }
  .sub { margin: 34px 0 0; font-size: 20px; color: var(--muted); letter-spacing: .14em; text-transform: uppercase; }
  .sub b { color: var(--cream); font-weight: 500; }
  .url { margin: 26px 0 0; font: 500 26px ui-monospace, Menlo, monospace; color: var(--ink); padding: 12px 22px; border-radius: 14px; background: rgba(255,255,255,.06); border: 1px solid rgba(255,255,255,.14); display: inline-block; }
  .cap { display: inline-flex; align-items: center; gap: 18px; padding: 16px 28px 16px 18px; border-radius: 999px; background: rgba(6,7,10,.78); border: 1px solid rgba(255,255,255,.18); box-shadow: 0 20px 60px rgba(0,0,0,.5); backdrop-filter: blur(16px); }
  .cap .n { width: 40px; height: 40px; border-radius: 50%; display: grid; place-items: center; font: 600 18px Inter; color: #0a0b0f; background: linear-gradient(120deg, var(--cream), #ffd9c9 50%, #bcd0ff); flex: none; }
  .cap .t { font: 500 26px/1.25 Inter; color: var(--ink); letter-spacing: -0.005em; white-space: nowrap; }
  .cap .t em { font-family: "Instrument Serif", Georgia, serif; font-style: italic; font-size: 1.12em; color: var(--cream); }
  .cap.fast .n { background: linear-gradient(120deg, #bcd0ff, #dfe7ff 50%, var(--cream)); }
</style>`;

const browser = await chromium.launch();
const page = await browser.newPage({ viewport: { width: W, height: H }, deviceScaleFactor: 1 });

async function card(name, html) {
  await page.setContent(`<html><head>${base}</head><body><div class="card"><i class="blob b1"></i><i class="blob b2"></i><i class="blob b3"></i><i class="disc"></i><i class="grain"></i><div class="center"><div>${html}</div></div></div></body></html>`);
  await page.evaluate(() => document.fonts.ready);
  await page.waitForTimeout(300);
  await page.locator('.card').screenshot({ path: `${OUT}/${name}.png` });
}
await card('intro', `<span class="eclipse"></span><h1>Antumbra</h1><p class="tag">a shielded pool with batch swaps on NEAR</p><p class="sub">Groth16 on BLS12-381 &nbsp;·&nbsp; sealed amounts &nbsp;·&nbsp; NEAR Intents</p>`);
await card('outro', `<span class="eclipse"></span><h1>Antumbra</h1><p class="tag">private transfers, sealed swaps, one price per epoch</p><p class="url">github.com/Valentine1898/antumbra</p><p class="sub">built at <b>NEAR HACKATHON</b></p>`);

const captions = JSON.parse(process.env.CAPTIONS);
const meta = {};
for (const [name, { n, text, fast }] of Object.entries(captions)) {
  await page.setContent(`<html><head>${base}</head><body><div class="cap${fast ? ' fast' : ''}"><span class="n">${n}</span><span class="t">${text}</span></div></body></html>`);
  await page.evaluate(() => document.fonts.ready);
  await page.waitForTimeout(150);
  const el = page.locator('.cap');
  await el.screenshot({ path: `${OUT}/cap-${name}.png`, omitBackground: true });
  meta[name] = await el.boundingBox();
}
writeFileSync(`${OUT}/captions.json`, JSON.stringify(meta, null, 2));
await browser.close();
console.log('overlays rendered:', Object.keys(captions).length, 'captions');
