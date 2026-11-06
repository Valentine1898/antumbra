// Records the animated intro and outro cards (the app's own motion language:
// drifting blobs, the eclipse glyph, type that rises out of the shade).
import { chromium } from 'playwright';
import { mkdirSync, readdirSync, renameSync } from 'node:fs';

const OUT = process.env.OUT;
const W = Number(process.env.W ?? 1920), H = Number(process.env.H ?? 1080);
mkdirSync(OUT, { recursive: true });

const css = `
<link href="https://fonts.googleapis.com/css2?family=Inter:wght@400;500;600;700&family=Instrument+Serif:ital@0;1&display=swap" rel="stylesheet">
<style>
  :root { --bg:#06070a; --ink:#f1f2f5; --muted:#8d93a3; --orange:#ff6a3d; --blue:#4f7dff; --cream:#f6efe6; }
  * { box-sizing: border-box; } html, body { margin: 0; background: var(--bg); overflow: hidden; }
  body { font: 15px/1.5 Inter, system-ui, sans-serif; color: var(--ink); -webkit-font-smoothing: antialiased; }
  .card { position: relative; width: ${W}px; height: ${H}px; overflow: hidden; background: var(--bg); }
  .blob { position: absolute; width: 70vmax; height: 70vmax; border-radius: 50%; filter: blur(80px); opacity: .5; mix-blend-mode: screen; }
  .b1 { left: -20vmax; top: -25vmax; background: radial-gradient(circle at 40% 40%, var(--orange) 0%, rgba(255,106,61,.35) 30%, transparent 62%); animation: drift-a 14s ease-in-out infinite alternate; }
  .b2 { right: -25vmax; top: -10vmax; background: radial-gradient(circle at 50% 50%, var(--blue) 0%, rgba(79,125,255,.35) 32%, transparent 64%); animation: drift-b 18s ease-in-out infinite alternate; }
  .b3 { left: 20vmax; bottom: -35vmax; width: 60vmax; height: 60vmax; opacity: .28; background: radial-gradient(circle at 50% 50%, var(--cream) 0%, rgba(246,239,230,.25) 28%, transparent 60%); animation: drift-c 16s ease-in-out infinite alternate; }
  @keyframes drift-a { from { transform: translate(0,0) scale(1); } to { transform: translate(10vmax, 7vmax) scale(1.1); } }
  @keyframes drift-b { from { transform: translate(0,0) scale(1.05); } to { transform: translate(-12vmax, 9vmax) scale(.95); } }
  @keyframes drift-c { from { transform: translate(0,0); } to { transform: translate(-8vmax, -10vmax); } }
  .disc { position: absolute; left: 50%; top: 50%; width: 62vmin; height: 62vmin; margin: -31vmin 0 0 -31vmin; border-radius: 50%; background: radial-gradient(circle at 50% 50%, #05060a 0 58%, rgba(5,6,10,.75) 62%, transparent 72%); animation: breathe 6s ease-in-out infinite; }
  @keyframes breathe { 0%,100% { transform: scale(1); } 50% { transform: scale(1.05); } }
  .grain { position: absolute; inset: 0; opacity: .16; mix-blend-mode: overlay; background-image: url("data:image/svg+xml;utf8,<svg xmlns='http://www.w3.org/2000/svg' width='220' height='220'><filter id='n'><feTurbulence type='fractalNoise' baseFrequency='0.9' numOctaves='2' stitchTiles='stitch'/><feColorMatrix values='0 0 0 0 1 0 0 0 0 1 0 0 0 0 1 0 0 0 0.6 0'/></filter><rect width='100%' height='100%' filter='url(%23n)'/></svg>"); }
  .center { position: absolute; inset: 0; display: grid; place-items: center; text-align: center; }
  .eclipse { width: 54px; height: 54px; border-radius: 50%; display: inline-block; margin-bottom: 26px;
    background: radial-gradient(circle at 50% 50%, #06070a 0 54%, transparent 57%), conic-gradient(from 210deg, var(--orange), var(--blue), var(--cream), var(--orange));
    box-shadow: 0 0 40px rgba(255,106,61,.5), 0 0 90px rgba(79,125,255,.3); animation: pop .9s cubic-bezier(.2,.8,.2,1) both, spin 12s linear 1s infinite; }
  @keyframes pop { from { opacity: 0; transform: scale(.4); filter: blur(8px); } to { opacity: 1; transform: none; filter: none; } }
  @keyframes spin { to { transform: rotate(360deg); } }
  h1 { margin: 0; font-size: 112px; font-weight: 600; letter-spacing: -0.04em; line-height: 1; animation: rise 1s cubic-bezier(.2,.8,.2,1) .35s both; }
  .tag { margin: 22px 0 0; font-family: "Instrument Serif", Georgia, serif; font-style: italic; font-size: 44px; color: var(--cream); opacity: .95; animation: rise 1s cubic-bezier(.2,.8,.2,1) .8s both; }
  .sub { margin: 34px 0 0; font-size: 20px; color: var(--muted); letter-spacing: .14em; text-transform: uppercase; animation: rise 1s cubic-bezier(.2,.8,.2,1) 1.25s both; }
  .sub b { color: var(--cream); font-weight: 500; }
  .url { margin: 26px 0 0; font: 500 26px ui-monospace, Menlo, monospace; color: var(--ink); padding: 12px 22px; border-radius: 14px; background: rgba(255,255,255,.06); border: 1px solid rgba(255,255,255,.14); display: inline-block; animation: rise 1s cubic-bezier(.2,.8,.2,1) 1.1s both; }
  @keyframes rise { from { opacity: 0; transform: translateY(26px); filter: blur(10px); letter-spacing: .02em; } to { opacity: 1; transform: none; filter: none; } }
</style>`;

async function record(name, html, seconds) {
  const browser = await chromium.launch();
  const ctx = await browser.newContext({ viewport: { width: W, height: H }, recordVideo: { dir: OUT, size: { width: W, height: H } } });
  const page = await ctx.newPage();
  await page.setContent(`<html><head>${css}</head><body><div class="card"><i class="blob b1"></i><i class="blob b2"></i><i class="blob b3"></i><i class="disc"></i><i class="grain"></i><div class="center"><div>${html}</div></div></div></body></html>`);
  await page.evaluate(() => document.fonts.ready);
  await page.waitForTimeout(seconds * 1000);
  const path = await page.video().path();
  await ctx.close();
  await browser.close();
  renameSync(path, `${OUT}/${name}.webm`);
  console.log('recorded', name);
}
await record('intro', `<span class="eclipse"></span><h1>Antumbra</h1><p class="tag">a shielded pool with batch swaps on NEAR</p><p class="sub">Groth16 on BLS12-381 &nbsp;·&nbsp; sealed amounts &nbsp;·&nbsp; NEAR Intents</p>`, 7);
await record('outro', `<span class="eclipse"></span><h1>Antumbra</h1><p class="tag">private transfers, sealed swaps, one price per epoch</p><p class="url">github.com/Valentine1898/antumbra</p><p class="sub">built at <b>NEAR HACKATHON</b></p>`, 6.5);
