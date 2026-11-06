// Records the Antumbra demo flow in a Chromium driven by Playwright.
// The gas payer is pre-seeded into localStorage so the video opens connected.
import { chromium } from 'playwright';
import { readFileSync, writeFileSync } from 'node:fs';

const S = process.env.S;
const URL = process.env.URL ?? 'http://localhost:5173/';
const acct = JSON.parse(readFileSync(process.env.ACCOUNT_FILE ?? `${S}/demo-account.secret.json`, 'utf8'));
const W = Number(process.env.W ?? 1920), H = Number(process.env.H ?? 1080);

const browser = await chromium.launch({ headless: true });
const ctx = await browser.newContext({
  viewport: { width: W, height: H },
  deviceScaleFactor: 1,
  colorScheme: 'dark',
  recordVideo: { dir: process.env.VIDEO_DIR ?? `${S}/video`, size: { width: W, height: H } },
});
const page = await ctx.newPage();
const t0 = Date.now();
page.on('framenavigated', (f) => { if (f === page.mainFrame()) console.log(`${((Date.now() - t0) / 1000).toFixed(1)}s navigated ${f.url()}`); });
page.on('load', () => console.log(`${((Date.now() - t0) / 1000).toFixed(1)}s load`));
page.on('console', (m) => { if (m.type() === 'error' || m.type() === 'warning' || m.text().includes('[vite]')) console.log(`${((Date.now() - t0) / 1000).toFixed(1)}s console.${m.type()} ${m.text().slice(0, 160)}`); });
page.on('pageerror', (e) => console.log(`${((Date.now() - t0) / 1000).toFixed(1)}s pageerror ${String(e).slice(0, 160)}`));
const markers = {};
const mark = (name) => { markers[name] = (Date.now() - t0) / 1000; console.log(`${markers[name].toFixed(1)}s ${name}`); };
const pause = (ms) => page.waitForTimeout(ms);

await page.addInitScript(({ account, key }) => {
  if (!localStorage.getItem('antumbra.signer')) {
    localStorage.setItem('antumbra.signer', JSON.stringify({ kind: 'devkey', account, key }));
    localStorage.setItem('antumbra.mode', 'calm');
  }
}, { account: acct.account, key: acct.key });

// A visible pointer: headless video has none, so the page draws one that follows the mouse events.
await page.addInitScript(() => {
  const init = () => {
    const c = document.createElement('div');
    c.id = 'demo-cursor';
    c.innerHTML = '<svg width="26" height="32" viewBox="0 0 26 32"><path d="M3 2 L3 24 L9 18 L13 28 L17 26 L13 16.5 L21 16.5 Z" fill="#f6efe6" stroke="#06070a" stroke-width="1.6" stroke-linejoin="round"/></svg><i></i>';
    Object.assign(c.style, { position: 'fixed', left: '0px', top: '0px', zIndex: '2147483647', pointerEvents: 'none', transform: 'translate(-3px,-2px)', filter: 'drop-shadow(0 2px 6px rgba(0,0,0,.6))', opacity: '0', transition: 'opacity .3s' });
    const ripple = c.querySelector('i');
    Object.assign(ripple.style, { position: 'absolute', left: '-14px', top: '-16px', width: '40px', height: '40px', borderRadius: '50%', border: '2px solid #ff6a3d', opacity: '0', pointerEvents: 'none' });
    document.body.appendChild(c);
    document.addEventListener('mousemove', (e) => { c.style.left = e.clientX + 'px'; c.style.top = e.clientY + 'px'; c.style.opacity = '1'; }, true);
    document.addEventListener('mousedown', () => { ripple.animate([{ transform: 'scale(.3)', opacity: .9 }, { transform: 'scale(1.5)', opacity: 0 }], { duration: 480, easing: 'ease-out' }); }, true);
  };
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', init); else init();
});

// Human-ish pointer: glide to the element, then click.
let cur = { x: W / 2, y: H / 2 };
async function glide(locator, opts = {}) {
  await locator.waitFor({ state: 'visible' });
  const box = await locator.boundingBox();
  const x = box.x + box.width * (opts.fx ?? 0.5), y = box.y + box.height * (opts.fy ?? 0.5);
  await page.mouse.move(cur.x, cur.y);
  await page.mouse.move(x, y, { steps: 28 });
  cur = { x, y };
  await pause(opts.settle ?? 350);
}
async function click(locator, opts) { await glide(locator, opts); await page.mouse.click(cur.x, cur.y); }
async function type(locator, text) { await click(locator); await page.keyboard.type(text, { delay: 90 }); }
const logEntry = (text, timeout = 90_000) => page.locator('#log li', { hasText: text }).first().waitFor({ timeout });
const lastLogs = async () => (await page.locator('#log li').allTextContents()).slice(0, 4).map((t) => t.slice(0, 160));
/// Waits until the wallet has caught up with the chain after a transaction (sync, then the delayed second sync).
async function settled() {
  await page.locator('#sync-status', { hasText: 'in sync' }).waitFor({ timeout: 30_000 });
  await pause(5500);
  await page.locator('#sync-status', { hasText: 'in sync' }).waitFor({ timeout: 30_000 });
}

try {
await page.goto(URL + '#threshold', { waitUntil: 'networkidle' });
await page.locator('#light-rows .brow').nth(1).waitFor();
await page.locator('#light-rows .brow .amt').nth(1).filter({ hasNotText: '·' }).waitFor({ timeout: 30_000 });
mark('open');
await page.mouse.move(W * 0.55, H * 0.55);
await pause(3200);

mark('wrap_click');
await click(page.getByRole('button', { name: 'wrap 1 NEAR' }));
await logEntry('wrapped 1 NEAR');
mark('wrapped');
await pause(1600);
mark('mint_click');
await click(page.getByRole('button', { name: 'get 100 USDC.e' }));
await logEntry('minted 100 USDC.e');
mark('minted');
await pause(1800);

mark('shield_click');
await type(page.locator('#move-amount'), '0.5');
await pause(500);
await click(page.getByRole('button', { name: /^Shield/ }));
await logEntry('shielded 0.5 wNEAR: confirmed');
mark('shielded');
await pause(3800);

await click(page.locator('.pages a[data-page="swap"]'));
mark('swap_page');
await pause(1400);
await type(page.locator('#swap-amount'), '0.1');
await page.locator('#route-quote strong').waitFor({ timeout: 30_000 }).catch(() => {});
mark('quoted');
await pause(3600);
mark('swap_click');
await click(page.getByRole('button', { name: 'Seal and swap' }));
await logEntry('sealed swap of 0.1 wNEAR');
mark('swapped');
await pause(2200);

// Waiting for the epoch to end and the sealer to clear it: show the machinery meanwhile.
await click(page.locator('.mode .track'));
mark('geek');
await pause(4500);
await page.locator('#swaps .card', { hasText: 'claimed' }).first().waitFor({ timeout: 180_000 });
await logEntry('claimed', 10_000).catch(() => {});
mark('claimed');
await settled();
await click(page.locator('.mode .track'));
await pause(2600);

await click(page.locator('.pages a[data-page="threshold"]'));
mark('threshold_page');
await pause(1200);
await click(page.locator('.routes button[data-route="out"]'));
await pause(700);
await page.locator('#move-token').selectOption('2');
await pause(500);
// Unshield what the claim brought: the displayed amount rounds down, so it never exceeds the balance.
const shown = (await page.locator('#shade-rows .brow').nth(1).locator('.amt').textContent()).trim().replace(/,/g, '');
const out = shown.startsWith('<') ? '0.0001' : shown;
console.log('unshielding', out, 'USDC.e');
await page.locator('#move-amount').fill('');
await type(page.locator('#move-amount'), out);
await pause(500);
mark('unshield_click');
await click(page.getByRole('button', { name: /^Unshield/ }));
await logEntry(`unshielded ${out} USDC.e`, 60_000).catch(async (e) => { console.log('unshield did not confirm; last log entries:', await lastLogs()); throw e; });
mark('unshielded');
await pause(4200);
mark('end');
} catch (e) {
  console.log('take failed:', e.message?.split('\n')[0]);
  console.log('last log entries:', await lastLogs().catch(() => []));
  markers.failed = true;
} finally {
  const videoPath = await page.video().path();
  await ctx.close();
  await browser.close();
  writeFileSync(`${process.env.VIDEO_DIR ?? S + "/video"}/markers.json`, JSON.stringify({ videoPath, markers }, null, 2));
  console.log('video', videoPath);
}
