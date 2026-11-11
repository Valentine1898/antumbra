import './style.css';
import init, * as wasm from './wasm/antumbra_wasm.js';
import { Chain, loadDeployment, teePoolAvailable, type Deployment, type EpochInfo, type PairInfo } from './chain';
import { DevKeySigner, WalletSigner, createDevAccount, type Signer, type TxResult } from './signer';
import { getProvingKey, local, putProvingKey } from './storage';

/// Public testnet RPCs, tried in order; `?rpc=<url>` puts your own first. Each one limits requests per IP.
const rpcParam = new URLSearchParams(location.search).get('rpc');
const RPC_URLS = [...(rpcParam ? [rpcParam] : []), 'https://test.rpc.fastnear.com', 'https://near-testnet.drpc.org', 'https://rpc.testnet.near.org'];
const EXPLORER = 'https://testnet.nearblocks.io';
const GAS_POOL = 200;
/// What a clearing inside the TEE is sent with. OutLayer wants its worst-case
/// estimate up front (about 0.007 NEAR) and returns what it does not use to the pool.
const TEE_DEPOSIT = '10000000000000000000000';
/// A request the enclave never answers fails after two hundred blocks, about two
/// minutes; only then is an epoch asked for again.
const TEE_RETRY_MS = 150_000;
const GAS_FT = 100;
const EPOCH_BLOCKS = 20;
/// NEP-145 registration deposit; a token refunds whatever exceeds its minimum.
const STORAGE_DEPOSIT = '2500000000000000000000';
const ONE_NEAR = '1000000000000000000000000';

type Route = 'in' | 'out' | 'send';
type Page = 'threshold' | 'swap';
interface Token { assetId: number; account: string; symbol: string; decimals: number; wrapsNear: boolean }
interface SwapEntry { index: number; epoch: number; pair_id: number; direction: number; delta: string; position: number; claimed: boolean }

const $ = <T extends HTMLElement = HTMLElement>(id: string) => document.getElementById(id) as T;

let deployment: Deployment;
let chain: Chain;
let signer: Signer | null = null;
let walletJson: string | null = local.get('wallet');
let epochInfo: EpochInfo | null = null;
let pair: PairInfo | null = null;
let tokens: Token[] = [];
let route: Route = 'in';
let selectorBundle: Awaited<ReturnType<typeof WalletSigner.setup>> | null = null;
let sealerShown = false;
let claiming = false;
let tick = 0;
/// For each epoch of a pending swap: whether the sealer has cleared its batch.
const batchCleared = new Map<number, boolean>();
/// Whether this pool's epochs are cleared inside a TEE (OutLayer) rather than by an operator's sealer.
let teePool = false;
/// Epochs this page has asked the TEE to clear, and when.
const teeAsked = new Map<number, number>();
let teeBusy = false;
/// What the gas payer holds in public: token account → base units, plus NEAR.
let publicBalances: Record<string, string> = {};
let nearBalance: string | null = null;

// ---------- mode and pages ----------
function setMode(mode: 'calm' | 'geek') {
  document.body.dataset.mode = mode;
  ($('mode-toggle') as HTMLInputElement).checked = mode === 'geek';
  local.set('mode', mode);
}

function pageFromHash(): Page {
  return location.hash === '#swap' ? 'swap' : 'threshold';
}

function setPage(p: Page) {
  document.body.dataset.page = p;
  $('page-threshold').classList.toggle('hidden', p !== 'threshold');
  $('page-swap').classList.toggle('hidden', p !== 'swap');
  document.querySelectorAll<HTMLAnchorElement>('.pages a').forEach((a) => a.classList.toggle('active', a.dataset.page === p));
}

// ---------- log and toast ----------
function log(text: string, opts: { hash?: string; pub?: unknown; error?: boolean; geek?: boolean } = {}) {
  const li = document.createElement('li');
  if (opts.geek) li.className = 'geek-entry';
  const t = document.createElement('span');
  t.className = 't';
  t.textContent = new Date().toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', second: '2-digit' });
  const msg = document.createElement('div');
  msg.className = 'msg' + (opts.error ? ' err' : '');
  msg.textContent = text;
  if (opts.hash) {
    const a = document.createElement('a');
    a.href = `${EXPLORER}/txns/${opts.hash}`;
    a.target = '_blank';
    a.rel = 'noreferrer';
    a.className = 'geek';
    a.textContent = 'explorer ↗';
    msg.appendChild(a);
  }
  if (opts.pub !== undefined) {
    const p = document.createElement('span');
    p.className = 'pub';
    p.textContent = 'what the chain saw: ' + JSON.stringify(opts.pub, null, 1).replace(/\n\s*/g, ' ');
    msg.appendChild(p);
  }
  li.append(t, msg);
  $('log').prepend(li);
}

function fail(e: unknown) {
  const msg = e instanceof Error ? e.message : String(e);
  log(msg, { error: true });
  setToast('');
  console.error(e);
}

function setToast(text: string, working = false) {
  const t = $('toast');
  t.textContent = text;
  t.classList.toggle('working', working && !!text);
}

function seed(): Uint8Array {
  return crypto.getRandomValues(new Uint8Array(32));
}

function shortAddress(addr: string): string {
  const [tag, pk, enc] = addr.split(':');
  return `${tag}:${pk.slice(0, 6)}…${pk.slice(-4)}:${enc.slice(0, 4)}…${enc.slice(-4)}`;
}

// ---------- amounts: humans type decimals, the pool counts base units ----------
function toBase(text: string, decimals: number): string {
  const t = text.replace(/[\s,_]/g, '');
  if (!/^(\d+\.?\d*|\.\d+)$/.test(t)) throw new Error('enter a positive amount');
  const [whole = '', frac = ''] = t.split('.');
  if (frac.length > decimals) throw new Error(`${decimals} decimal places at most for this token`);
  const base = BigInt(whole || '0') * 10n ** BigInt(decimals) + BigInt(frac.padEnd(decimals, '0') || '0');
  if (base === 0n) throw new Error('enter a positive amount');
  return base.toString();
}

function fromBase(base: string | bigint, decimals: number, maxFrac = 4): string {
  const b = BigInt(base);
  const unit = 10n ** BigInt(decimals);
  const whole = (b / unit).toLocaleString('en-US');
  const frac = (b % unit).toString().padStart(decimals, '0').slice(0, maxFrac).replace(/0+$/, '');
  if (b > 0n && b < unit && frac === '') return `<0.${'0'.repeat(Math.max(maxFrac - 1, 0))}1`;
  return frac ? `${whole}.${frac}` : whole;
}

function token(assetId: number): Token {
  return tokens.find((t) => t.assetId === assetId) ?? { assetId, account: '?', symbol: `asset ${assetId}`, decimals: 0, wrapsNear: false };
}
function amountText(assetId: number, base: string): string {
  const t = token(assetId);
  return `${fromBase(base, t.decimals)} ${t.symbol}`;
}
function selectedToken(selectId: string): Token {
  return token(Number(($(selectId) as HTMLSelectElement).value));
}

/// NEP-148 metadata for the pool's tokens; test tokens without metadata get a plain name.
async function loadTokens() {
  tokens = await Promise.all(
    deployment.tokens.map(async (account, i) => {
      const meta = await chain.ftMetadata(account).catch(() => null);
      return { assetId: i + 1, account, symbol: meta?.symbol ?? `TEST${i + 1}`, decimals: meta?.decimals ?? 0, wrapsNear: /^wrap\.(testnet|near)$/.test(account) };
    }),
  );
}

// ---------- worker (proving) ----------
const worker = new Worker(new URL('./worker.ts', import.meta.url), { type: 'module' });
let nextId = 1;
const pending = new Map<number, { resolve: (v: any) => void; reject: (e: any) => void }>();
worker.onmessage = async (e) => {
  const m = e.data;
  if (m.type === 'progress') { $('keys-status').textContent = m.text; return; }
  if (m.type === 'generated') { await putProvingKey(m.name, m.bytes); log(`generated the ${m.name} proving key in ${(m.ms / 1000).toFixed(1)} s (cached in this browser)`, { geek: true }); return; }
  const p = pending.get(m.id);
  if (!p) return;
  pending.delete(m.id);
  if (m.error) p.reject(new Error(m.error)); else p.resolve(m);
};
function ask(msg: Record<string, unknown>): Promise<any> {
  const id = nextId++;
  return new Promise((resolve, reject) => {
    pending.set(id, { resolve, reject });
    worker.postMessage({ id, ...msg });
  });
}
let keysReady: Promise<void> | null = null;
function ensureKeys(): Promise<void> {
  if (!keysReady) {
    keysReady = (async () => {
      $('keys-status').textContent = 'loading proving keys…';
      const keys: Record<string, Uint8Array | null> = {};
      for (const n of ['joinsplit', 'swap', 'swapclaim']) keys[n] = await getProvingKey(n);
      await ask({ type: 'keys', keys });
      $('keys-status').textContent = 'proving keys ready · joinsplit, swap, swapclaim';
    })().catch((e) => { keysReady = null; throw e; });
  }
  return keysReady;
}
async function prepare(op: string, p: Record<string, unknown>): Promise<{ result: string; ms: number }> {
  await ensureKeys();
  return ask({ type: 'prepare', op, wallet: walletJson, p, seed: seed() });
}

// ---------- wallet state ----------
function saveWallet(json: string) {
  walletJson = json;
  local.set('wallet', json);
}

function ensureWallet() {
  if (!walletJson) saveWallet(wasm.wallet_new(seed()));
}

function summary(): any {
  return JSON.parse(wasm.wallet_summary(walletJson!));
}

/// A wallet's notes, swaps and leaf cache belong to one pool; on another pool they are
/// meaningless and the cache would even yield wrong Merkle paths. Keys are kept.
function forgetPoolState(reason: string) {
  const w = JSON.parse(walletJson!);
  w.notes = [];
  w.swaps = [];
  w.leaves = [];
  saveWallet(JSON.stringify(w));
  batchCleared.clear();
  lastRender = '';
  log(`${reason}: keys kept, notes and the leaf cache forgotten, rescanning this pool`, { geek: true });
}

/// Binds the wallet to the pool in deployment.json. A wallet that followed another pool
/// (a redeploy, or an imported file) is detected by its first cached leaf.
async function adoptPool() {
  const known = local.get('pool');
  if (known !== deployment.pool) {
    const w = JSON.parse(walletJson!);
    if (Array.isArray(w.leaves) && w.leaves.length > 0) {
      let stale = known !== null;
      if (!stale) {
        const first = await chain.leaves(0, 1).catch(() => null);
        stale = first !== null && (first.length === 0 || first[0].cm !== w.leaves[0]);
      }
      if (stale) forgetPoolState('this wallet followed another pool');
    }
    local.set('pool', deployment.pool);
  }
}

// ---------- the two realms: what NEAR sees of you, and what only you can see ----------
interface RealmRow { el: HTMLElement; amt: HTMLElement; extra: HTMLElement }
const lightRows = new Map<number, RealmRow>();
const shadeRows = new Map<number, RealmRow>();
let gasRow: RealmRow | null = null;

function realmRow(container: HTMLElement, cls: string, symbol: string): RealmRow {
  const el = document.createElement('div');
  el.className = `brow ${cls}`.trim();
  el.innerHTML = `<span class="amt"></span><span class="sym">${symbol}</span><span class="spacer"></span><span class="extra"></span>`;
  container.appendChild(el);
  return { el, amt: el.querySelector('.amt')!, extra: el.querySelector('.extra')! };
}

let lastActions = '';
function renderRealms() {
  // In the light: the gas payer's account and its public balances.
  $('light-account').textContent = signer ? signer.accountId : 'nobody is paying gas yet';
  $('light-account').title = signer?.accountId ?? '';
  const lightBox = $('light-rows');
  if (!gasRow) {
    gasRow = realmRow(lightBox, 'gas', 'NEAR');
    gasRow.extra.className = 'note';
    gasRow.extra.textContent = 'for gas';
  }
  gasRow.amt.textContent = signer && nearBalance ? fromBase(nearBalance, 24, 2) : '·';
  gasRow.el.classList.toggle('hidden', !signer);
  const shade = new Map<number, bigint>();
  if (walletJson) for (const x of summary().balances) shade.set(x.asset_id, BigInt(x.amount));
  const actionsKey = `${signer?.accountId ?? ''}|${tokens.map((t) => t.symbol).join()}`;
  const rebuildActions = actionsKey !== lastActions;
  lastActions = actionsKey;
  for (const t of tokens) {
    let row = lightRows.get(t.assetId);
    if (!row) { row = realmRow(lightBox, '', t.symbol); lightRows.set(t.assetId, row); }
    const lit = BigInt(publicBalances[t.account] ?? '0');
    row.amt.textContent = signer ? fromBase(lit, t.decimals) : '·';
    row.el.classList.toggle('empty', lit === 0n);
    if (rebuildActions) {
      row.extra.innerHTML = '';
      if (signer) {
        const btn = document.createElement('button');
        btn.className = 'ghost tiny';
        btn.textContent = t.wrapsNear ? 'wrap 1 NEAR' : `get 100 ${t.symbol}`;
        btn.addEventListener('click', () => void busy(btn, () => (t.wrapsNear ? wrapNear(t) : mintTest(t))));
        row.extra.appendChild(btn);
      }
    }
  }
  // In the shade: the shielded balances, with the share of each token that is here.
  const shadeBox = $('shade-rows');
  for (const t of tokens) {
    let row = shadeRows.get(t.assetId);
    if (!row) { row = realmRow(shadeBox, '', t.symbol); shadeRows.set(t.assetId, row); }
    const sh = shade.get(t.assetId) ?? 0n;
    const lit = BigInt(publicBalances[t.account] ?? '0');
    const total = lit + sh;
    row.amt.textContent = fromBase(sh, t.decimals);
    row.el.classList.toggle('empty', sh === 0n);
    row.extra.innerHTML = total > 0n && signer ? `<span class="share" title="share of your ${t.symbol} that is in the shade">${Math.round(Number((sh * 1000n) / total) / 10)}% here</span>` : '';
  }
}

async function refreshLight() {
  if (!signer) {
    nearBalance = null;
    publicBalances = {};
    renderRealms();
    return;
  }
  const [near, ...bals] = await Promise.all([
    chain.nearBalance(signer.accountId).catch(() => '0'),
    ...tokens.map((t) => chain.ftBalance(t.account, signer!.accountId).catch(() => '0')),
  ]);
  nearBalance = near;
  tokens.forEach((t, i) => { publicBalances[t.account] = bals[i]; });
  renderRealms();
}

let lastRender = '';
function renderWallet() {
  if (!walletJson) return;
  const summaryJson = wasm.wallet_summary(walletJson);
  const key = [summaryJson, epochInfo?.epoch, epochInfo?.blocks_left, pair?.pair_id, claiming, signer?.kind, teePool, [...teeAsked.keys()].join(','), [...batchCleared.entries()].join(','), tokens.map((t) => t.symbol).join()].join('|');
  if (key === lastRender) return;
  lastRender = key;
  const s = JSON.parse(summaryJson);
  $('address').textContent = s.address;
  $('address-short').textContent = shortAddress(s.address);
  renderRealms();

  const sw = $('swaps');
  sw.innerHTML = '';
  for (const x of s.swaps as SwapEntry[]) {
    const sold = x.direction === 1 ? (pair?.asset_1 ?? 1) : (pair?.asset_2 ?? 2);
    const bought = x.direction === 1 ? (pair?.asset_2 ?? 2) : (pair?.asset_1 ?? 1);
    const card = document.createElement('div');
    card.className = 'card';
    let chip: string;
    let note = '';
    if (x.claimed) chip = '<span class="chip ok">claimed</span>';
    else if (!epochInfo || x.epoch >= epochInfo.epoch) { chip = '<span class="chip hot">sealed</span>'; note = epochInfo ? `epoch ends in ${epochInfo.blocks_left} blocks` : ''; }
    else if (!batchCleared.get(x.epoch)) {
      chip = '<span class="chip hot">sealed</span>';
      if (!teePool) note = 'epoch over, the sealer clears it within seconds to a few minutes';
      else if (teeAsked.has(x.epoch)) note = 'epoch over, the TEE is clearing it';
      else note = signer?.kind === 'devkey' ? 'epoch over, asking the TEE to clear it' : 'epoch over, ask the TEE to clear it';
    }
    else if (claiming) chip = '<span class="chip">claiming…</span>';
    else { chip = '<span class="chip ok">cleared</span>'; note = signer?.kind === 'devkey' ? 'claiming shortly' : 'ready to claim'; }
    card.innerHTML = `${chip}<span class="what">${amountText(sold, x.delta)} → ${token(bought).symbol}<span class="muted"> · epoch ${x.epoch}${note ? ' · ' + note : ''}</span></span>`;
    if (!x.claimed && batchCleared.get(x.epoch) && !claiming) {
      const btn = document.createElement('button');
      btn.className = 'ghost tiny';
      btn.textContent = 'Claim';
      btn.addEventListener('click', () => void busy(btn, () => claimSwap(x)));
      card.appendChild(btn);
    }
    // With a wallet every call is confirmed by hand, so on a TEE pool the clearing gets a button of its own.
    if (teePool && !x.claimed && epochInfo && x.epoch < epochInfo.epoch && batchCleared.get(x.epoch) === false && signer?.kind !== 'devkey' && !teeAsked.has(x.epoch)) {
      const btn = document.createElement('button');
      btn.className = 'ghost tiny';
      btn.textContent = 'Clear via TEE';
      btn.addEventListener('click', () => void busy(btn, () => clearViaTee(x.epoch)));
      card.appendChild(btn);
    }
    sw.appendChild(card);
  }

  const nt = $('notes-table').querySelector('tbody')!;
  nt.innerHTML = s.notes.length ? '' : '<tr class="empty"><td colspan="5">no notes yet</td></tr>';
  for (const n of s.notes) {
    const tr = document.createElement('tr');
    if (n.spent) tr.className = 'spent';
    tr.innerHTML = `<td>${n.index}</td><td>${token(n.asset_id).symbol}</td><td title="${n.amount} base units">${fromBase(n.amount, token(n.asset_id).decimals, 6)}</td><td>${n.position}</td><td>${n.spent ? 'spent' : 'unspent'}</td>`;
    nt.appendChild(tr);
  }
  const st = $('swaps-table').querySelector('tbody')!;
  st.innerHTML = s.swaps.length ? '' : '<tr class="empty"><td colspan="6">no swaps yet</td></tr>';
  for (const x of s.swaps as SwapEntry[]) {
    const sold = x.direction === 1 ? (pair?.asset_1 ?? 1) : (pair?.asset_2 ?? 2);
    const tr = document.createElement('tr');
    tr.innerHTML = `<td>${x.index}</td><td>${x.epoch}</td><td>${x.direction === 1 ? '1→2' : '2→1'}</td><td title="${x.delta} base units">${amountText(sold, x.delta)}</td><td>${x.position}</td><td>${x.claimed ? 'claimed' : 'open'}</td>`;
    st.appendChild(tr);
  }
}

async function sync(verbose = true) {
  ensureWallet();
  $('sync').classList.add('spinning');
  $('sync-status').textContent = 'syncing…';
  try {
    const total = await chain.leafCount();
    let from = summary().leaves as number;
    if (from > total) {
      forgetPoolState('the leaf cache is ahead of the pool');
      from = 0;
    }
    let newNotes = 0, newSwaps = 0;
    while (from < total) {
      const page = await chain.leaves(from, 50);
      if (page.length === 0) break;
      const r = JSON.parse(wasm.scan(walletJson!, JSON.stringify(page)));
      saveWallet(r.wallet);
      newNotes += r.report.new_notes; newSwaps += r.report.new_swaps;
      from = page[page.length - 1].position + 1;
    }
    const nf = JSON.parse(wasm.nullifiers(walletJson!));
    const spentNotes: number[] = [], claimedSwaps: number[] = [];
    for (const [i, h] of nf.notes) if (await chain.isSpent(h)) spentNotes.push(i);
    for (const [i, h] of nf.swaps) if (await chain.isSpent(h)) claimedSwaps.push(i);
    if (spentNotes.length || claimedSwaps.length) saveWallet(wasm.apply_spent(walletJson!, JSON.stringify(spentNotes), JSON.stringify(claimedSwaps)));
    $('sync-status').textContent = `in sync · ${total} leaves`;
    if (verbose) log(`synced: ${newNotes} new notes, ${newSwaps} new swaps, ${spentNotes.length} spent, ${claimedSwaps.length} claimed`, { geek: true });
  } catch (e) {
    $('sync-status').textContent = 'sync failed';
    fail(e);
  } finally {
    $('sync').classList.remove('spinning');
  }
  renderWallet();
}

/// Syncs now and once more a few seconds later: a public RPC node can lag a block behind the transaction.
async function syncSoon() {
  await sync(false);
  setTimeout(() => void sync(false), 4000);
}

// ---------- the light: getting tokens into the gas payer's account ----------
/// NEP-145: a token only holds balances for registered accounts, so register before sending anyone tokens.
async function ensureRegistered(tokenAccount: string, who: string) {
  const s = requireSigner();
  if (await chain.storageBalance(tokenAccount, who)) return;
  const r = await s.call(tokenAccount, 'storage_deposit', { account_id: who, registration_only: true }, 30, STORAGE_DEPOSIT);
  if (r === 'redirect') throw new Error('the wallet is registering the account; try again when it returns');
  if (!r.ok) throw new Error(`could not register ${who} on ${tokenAccount}: ${r.error}`);
  log(`registered ${who} on ${tokenAccount} (NEP-145 storage)`, { hash: r.hash, geek: true });
}

async function wrapNear(t: Token) {
  const s = requireSigner();
  await ensureRegistered(t.account, s.accountId);
  setToast('Wrapping 1 NEAR…', true);
  const r = await s.call(t.account, 'near_deposit', {}, 30, ONE_NEAR);
  if (r === 'redirect') return;
  reportTx(`wrapped 1 NEAR into ${t.symbol}`, r);
  await refreshLight();
}

async function mintTest(t: Token) {
  const s = requireSigner();
  await ensureRegistered(t.account, s.accountId);
  setToast(`Minting 100 ${t.symbol}…`, true);
  const r = await s.call(t.account, 'mint', { account_id: s.accountId, amount: toBase('100', t.decimals) }, 30, '0');
  if (r === 'redirect') return;
  reportTx(`minted 100 ${t.symbol} from the test faucet`, r);
  await refreshLight();
}

// ---------- pool ----------
async function refreshBatches() {
  if (!walletJson || !epochInfo) return;
  const epochs = new Set<number>();
  for (const x of summary().swaps as SwapEntry[]) if (!x.claimed && x.epoch < epochInfo.epoch && !batchCleared.get(x.epoch)) epochs.add(x.epoch);
  for (const ep of epochs) {
    const b = await chain.getBatch(ep, deployment.pair_id);
    batchCleared.set(ep, !!b?.executed);
  }
}

function updateSwapBuy() {
  if (!pair || tokens.length === 0) return;
  const sell = selectedToken('swap-sell');
  const buy = sell.assetId === pair.asset_1 ? pair.asset_2 : pair.asset_1;
  $('swap-buy').textContent = token(buy).symbol;
}

// ---------- NEAR Intents (1Click): what the batch's net leg would fetch on mainnet ----------
const ONECLICK = 'https://1click.chaindefuser.com/v0';
let oneClickTokens: Promise<any[]> | null = null;
let quoteTimer: number | undefined;
let quoteSeq = 0;

/// The mainnet 1Click asset for one of the pool's tokens, by symbol (USDC.e stands in for USDC).
async function oneClickAsset(sym: string): Promise<any | null> {
  oneClickTokens ??= fetch(`${ONECLICK}/tokens`).then((r) => r.json()).catch(() => []);
  const want = sym.replace(/\.e$/, '').toUpperCase();
  return (await oneClickTokens).find((t) => t.blockchain === 'near' && String(t.symbol).toUpperCase() === want) ?? null;
}

/// Live dry quote for the amount being typed, the way a NEAR-resident pool would route a batch's
/// net leg: Intents balance to Intents balance. Mainnet prices; nothing moves from the testnet pool.
async function routeQuote() {
  const el = $('route-quote');
  const seq = ++quoteSeq;
  try {
    if (!pair) return;
    const sell = selectedToken('swap-sell');
    const buy = token(sell.assetId === pair.asset_1 ? pair.asset_2 : pair.asset_1);
    const human = ($('swap-amount') as HTMLInputElement).value.trim();
    let amount: string;
    try { amount = toBase(human, sell.decimals); } catch { el.textContent = ''; return; }
    const [a, b] = await Promise.all([oneClickAsset(sell.symbol), oneClickAsset(buy.symbol)]);
    if (!a || !b) { el.textContent = ''; return; }
    el.textContent = 'asking NEAR Intents…';
    const deadline = new Date(Date.now() + 3600_000).toISOString();
    const res = await fetch(`${ONECLICK}/quote`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        dry: true, swapType: 'EXACT_INPUT', slippageTolerance: 100,
        originAsset: a.assetId, depositType: 'INTENTS', destinationAsset: b.assetId, amount,
        recipient: 'antumbra.near', recipientType: 'INTENTS', refundTo: 'antumbra.near', refundType: 'INTENTS', deadline,
      }),
    });
    const body = await res.json();
    if (seq !== quoteSeq) return;
    if (!res.ok || !body.quote) throw new Error(body.message ?? 'no quote');
    const q = body.quote;
    const out = fromBase(q.amountOut, b.decimals, 4);
    el.innerHTML = `On mainnet the batch's net leg would clear through NEAR Intents: ${human} ${sell.symbol} → <strong>${out} ${b.symbol}</strong> in about ${q.timeEstimate} s <span class="muted">(live 1Click quote, nothing moves on testnet)</span>`;
  } catch (e) {
    if (seq === quoteSeq) el.innerHTML = `<span class="muted">NEAR Intents has no quote right now (${e instanceof Error ? e.message : e})</span>`;
  }
}

function scheduleRouteQuote() {
  clearTimeout(quoteTimer);
  quoteTimer = window.setTimeout(() => void routeQuote(), 600);
}

async function refreshPool() {
  try {
    const [e, p, root, leaves] = await Promise.all([chain.currentEpoch(), chain.getPair(deployment.pair_id), chain.getRoot(), chain.leafCount()]);
    epochInfo = e;
    pair = p;
    $('epoch').textContent = String(e.epoch);
    $('epoch-left').textContent = `${e.blocks_left} blocks left`;
    $('epoch-bar').style.width = `${Math.round(((EPOCH_BLOCKS - e.blocks_left) / EPOCH_BLOCKS) * 100)}%`;
    $('swap-epoch').textContent = String(e.epoch);
    $('swap-left').textContent = String(e.blocks_left);
    $('epoch-pill').textContent = `epoch ${e.epoch} · ${e.blocks_left} blocks left`;
    $('pool-root').textContent = root;
    $('pool-root').title = root;
    $('leaves').textContent = String(leaves);
    if (p) $('pair').textContent = `${amountText(p.asset_1, p.reserve_1)} / ${amountText(p.asset_2, p.reserve_2)}`;
    const batch = await chain.getBatch(e.epoch, deployment.pair_id);
    $('batch').textContent = batch ? `epoch ${e.epoch}: ${batch.entries} sealed swap${batch.entries === 1 ? '' : 's'}, amounts hidden until the sealer clears it` : `epoch ${e.epoch}: empty so far`;
    if (!sealerShown) {
      const sealer = await chain.getSealer();
      $('sealer').textContent = sealer ? `${sealer.pk[0].slice(0, 10)}… (${sealer.outlayer ? 'OutLayer TEE' : 'signed executor'})` : 'none registered';
      sealerShown = true;
      teePool = !!sealer?.outlayer;
      void renderPoolSwitch();
    }
    await refreshBatches();
    void autoClearViaTee();
    updateSwapBuy();
    renderWallet();
    if (tick++ % 3 === 0) void refreshLight();
    void autoClaim();
  } catch (e) {
    console.error(e);
  }
}

// ---------- signing helpers ----------
function requireSigner(): Signer {
  if (!signer) {
    ($('signer-dialog') as HTMLDialogElement).showModal();
    throw new Error('choose who pays gas first');
  }
  return signer;
}

async function sendPrepared(prepared: any, describe: string): Promise<boolean> {
  const s = requireSigner();
  local.set('pending', JSON.stringify({ prepared, describe }));
  setToast('Waiting for the chain…', true);
  const r = await s.call(deployment.pool, prepared.method, prepared.args, GAS_POOL, '0');
  if (r === 'redirect') return false;
  local.del('pending');
  reportTx(describe, r, prepared.public_view);
  if (r.ok) saveWallet(wasm.apply_prepared(walletJson!, JSON.stringify(prepared)));
  await syncSoon();
  return r.ok;
}

function reportTx(describe: string, r: TxResult, pub?: unknown) {
  if (r.ok) {
    setToast('Done. The chain saw nothing it should not.');
    log(`${describe}: confirmed`, { hash: r.hash, pub });
    setTimeout(() => setToast(''), 5000);
  } else {
    setToast('');
    log(`${describe}: failed: ${r.error}`, { hash: r.hash, error: true });
  }
}

async function busy<T>(btn: HTMLButtonElement, f: () => Promise<T>): Promise<T | undefined> {
  btn.disabled = true;
  try { return await f(); } catch (e) { fail(e); return undefined; } finally { btn.disabled = false; }
}

async function proveAndSend(op: string, params: Record<string, unknown>, describe: string): Promise<boolean> {
  requireSigner();
  setToast('Proving in your browser…', true);
  const { result, ms } = await prepare(op, params);
  log(`${op} proof built in ${(ms / 1000).toFixed(1)} s, inside a Web Worker`, { geek: true });
  return sendPrepared(JSON.parse(result), describe);
}

/// A mote of value crossing the terminator.
function crossing(dir: 'in' | 'out') {
  const el = $('crossing');
  el.classList.remove('go-in', 'go-out');
  void el.offsetWidth;
  el.classList.add(dir === 'in' ? 'go-in' : 'go-out');
}

// ---------- the threshold: into, out of, or within the shade ----------
/// NEAR's account id rule: lowercase alphanumeric parts joined by single `-`, `_` or `.`.
const NEAR_ACCOUNT_ID = /^(([a-z\d]+[-_])*[a-z\d]+\.)*([a-z\d]+[-_])*[a-z\d]+$/;
const ROUTES: Record<Route, { hint: string; verb: string; to: string | null }> = {
  in: { hint: 'Public: the amount and a note commitment. <em>Hidden: who owns the note.</em>', verb: 'Shield', to: null },
  out: { hint: 'Public: the amount and the receiving account. <em>Hidden: which note paid.</em>', verb: 'Unshield', to: 'To NEAR account' },
  send: { hint: 'Public: two nullifiers, two commitments. <em>Hidden: sender, recipient, amount.</em>', verb: 'Send in the shade', to: 'To shielded address' },
};

function setRoute(r: Route) {
  // The recipient field means a NEAR account on one route and a shielded address on another: never carry it over.
  if (r !== route) ($('move-to') as HTMLInputElement).value = '';
  route = r;
  document.querySelectorAll<HTMLButtonElement>('.routes button').forEach((b) => b.classList.toggle('active', b.dataset.route === r));
  $('move-hint').innerHTML = ROUTES[r].hint;
  $('move-to-wrap').classList.toggle('hidden', !ROUTES[r].to);
  $('move-to-label').textContent = ROUTES[r].to ?? '';
  ($('move-to') as HTMLInputElement).placeholder = r === 'send' ? 'antumbra:…' : (signer?.accountId ?? 'name.testnet');
  $('form-move').dataset.route = r;
  updateMoveButton();
}

function updateMoveButton() {
  const v = ($('move-amount') as HTMLInputElement).value.trim();
  const sym = tokens.length ? selectedToken('move-token').symbol : '';
  $('move-submit').textContent = v ? `${ROUTES[route].verb} ${v} ${sym}` : ROUTES[route].verb;
}

async function doMove(btn: HTMLButtonElement) {
  await busy(btn, async () => {
    const s = requireSigner();
    const t = selectedToken('move-token');
    const human = ($('move-amount') as HTMLInputElement).value.trim();
    const amount = toBase(human, t.decimals);
    const to = ($('move-to') as HTMLInputElement).value.trim();
    if (route === 'in') {
      const d = JSON.parse(wasm.prepare_deposit(walletJson!, t.assetId, amount, seed()));
      const describe = `shielded ${human} ${t.symbol}`;
      local.set('pending', JSON.stringify({ describe }));
      setToast('Waiting for the chain…', true);
      const r = await s.call(t.account, 'ft_transfer_call', { receiver_id: deployment.pool, amount, msg: d.msg }, GAS_FT, '1');
      if (r === 'redirect') return;
      local.del('pending');
      reportTx(describe, r, { amount, commitment: d.commitment });
      if (r.ok) crossing('in');
      await syncSoon();
      void refreshLight();
    } else if (route === 'out') {
      const dest = to || s.accountId;
      if (dest.startsWith('antumbra:')) throw new Error('that is a shielded address: send to it with "To another shade", or enter a NEAR account here');
      if (dest.length < 2 || dest.length > 64 || !NEAR_ACCOUNT_ID.test(dest)) throw new Error(`"${dest}" is not a NEAR account id`);
      await ensureRegistered(t.account, dest);
      const ok = await proveAndSend('withdraw', { asset_id: t.assetId, amount, to: dest }, `unshielded ${human} ${t.symbol} to ${dest}`);
      if (ok) crossing('out');
      void refreshLight();
    } else {
      if (!to.startsWith('antumbra:')) throw new Error('enter a shielded address (antumbra:…)');
      await proveAndSend('transfer', { asset_id: t.assetId, amount, to }, `sent ${human} ${t.symbol} within the shade`);
    }
  });
}

// ---------- swaps ----------
async function doSwap(btn: HTMLButtonElement) {
  await busy(btn, async () => {
    requireSigner();
    if (!pair) throw new Error('pair not loaded yet');
    const t = selectedToken('swap-sell');
    const human = ($('swap-amount') as HTMLInputElement).value.trim();
    const amount = toBase(human, t.decimals);
    const e = await chain.currentEpoch();
    const sealer = await chain.getSealer();
    if (!sealer) throw new Error('the pool has no sealer registered yet');
    await proveAndSend('swap', { pair_id: pair.pair_id, asset_1: pair.asset_1, asset_2: pair.asset_2, sell_asset: t.assetId, amount, epoch: e.epoch, pk_x: sealer.pk[0], pk_y: sealer.pk[1] }, `sealed swap of ${human} ${t.symbol} in epoch ${e.epoch}`);
  });
}

async function claimSwap(sw: SwapEntry) {
  requireSigner();
  const p = pair ?? (await chain.getPair(deployment.pair_id));
  if (!p) throw new Error('pair not loaded');
  const batch = await chain.getBatch(sw.epoch, sw.pair_id);
  if (!batch?.executed) throw new Error(`epoch ${sw.epoch} is not cleared yet`);
  setToast('Proving your share in your browser…', true);
  const { result, ms } = await prepare('claim', { index: sw.index, asset_1: p.asset_1, asset_2: p.asset_2, batch: JSON.stringify(batch) });
  const r = JSON.parse(result);
  log(`claim proof for swap #${sw.index} built in ${(ms / 1000).toFixed(1)} s`, { geek: true });
  const out = r.outputs[0];
  const refund = Number(r.outputs[1].amount) ? ` plus a refund of ${amountText(r.outputs[1].asset_id, r.outputs[1].amount)}` : '';
  await sendPrepared(r.prepared, `claimed ${amountText(out.asset_id, out.amount)}${refund}`);
}

/// With a dev key there is nothing to confirm, so cleared swaps are claimed as soon as the sealer is done.
async function autoClaim() {
  if (claiming || !walletJson || signer?.kind !== 'devkey') return;
  const ready = (summary().swaps as SwapEntry[]).filter((x) => !x.claimed && batchCleared.get(x.epoch));
  if (ready.length === 0) return;
  claiming = true;
  renderWallet();
  try {
    for (const sw of ready) await claimSwap(sw);
  } catch (e) {
    fail(e);
  } finally {
    claiming = false;
    renderWallet();
  }
}

/// Asks the pool to have one finished epoch cleared inside the TEE. Any account
/// may: the pool sends its own batch to OutLayer and executes what comes back.
async function clearViaTee(epoch: number) {
  const s = requireSigner();
  teeAsked.set(epoch, Date.now());
  renderWallet();
  setToast('Asking the TEE to clear the epoch…', true);
  try {
    const r = await s.call(deployment.pool, 'end_epoch', { epoch, pair_id: deployment.pair_id }, GAS_POOL, TEE_DEPOSIT);
    if (r === 'redirect') return;
    if (r.ok) log(`epoch ${epoch} cleared inside the TEE`, { hash: r.hash });
    else if (!/already cleared/.test(r.error ?? '')) log(`the TEE did not clear epoch ${epoch}: ${r.error}`, { hash: r.hash, error: true });
  } catch (e) {
    // The call waits for the enclave, and an RPC endpoint may stop waiting
    // first. That is not a failure: the batch itself says whether it was cleared.
    console.warn('end_epoch', e);
  } finally {
    setToast('');
  }
  await refreshBatches();
  renderWallet();
  void autoClaim();
}

/// With a dev key there is nothing to confirm, so on a TEE pool the page asks
/// for the clearing of every finished epoch that holds one of its swaps.
async function autoClearViaTee() {
  if (!teePool || teeBusy || !walletJson || !epochInfo || signer?.kind !== 'devkey') return;
  const due = new Set<number>();
  for (const x of summary().swaps as SwapEntry[]) {
    if (x.claimed || x.epoch >= epochInfo.epoch || batchCleared.get(x.epoch) !== false) continue;
    if (Date.now() - (teeAsked.get(x.epoch) ?? 0) > TEE_RETRY_MS) due.add(x.epoch);
  }
  if (due.size === 0) return;
  teeBusy = true;
  try {
    for (const epoch of due) await clearViaTee(epoch);
  } catch (e) {
    fail(e);
  } finally {
    teeBusy = false;
  }
}

/// The Geek panel's button: clears the oldest finished epoch that holds one of
/// this wallet's swaps, or the last finished epoch when there is none.
async function doEndEpoch(btn: HTMLButtonElement) {
  await busy(btn, async () => {
    requireSigner();
    if (!teePool) throw new Error('this pool clears epochs through its signed sealer; nothing to trigger from here');
    const current = (await chain.currentEpoch()).epoch;
    const mine = (summary().swaps as SwapEntry[]).filter((x) => !x.claimed && x.epoch < current && !batchCleared.get(x.epoch)).map((x) => x.epoch);
    await clearViaTee(mine.length ? Math.min(...mine) : current - 1);
    await refreshPool();
  });
}

/// Says under the swap form who seals this pool, with a way to the other pool when the site serves both.
async function renderPoolSwitch() {
  $('end-epoch').classList.toggle('hidden', !teePool);
  $('pool-chip').classList.toggle('hidden', !teePool);
  const el = $('pool-switch');
  if (teePool) {
    el.innerHTML = `This pool's sealer runs inside a TEE: its key was generated in the enclave and nobody holds it. <a href="${location.pathname}#swap">The pool with an operator's sealer →</a>`;
  } else if (await teePoolAvailable()) {
    el.innerHTML = `This pool's sealer is run by the pool's operator. <a href="${location.pathname}?pool=tee#swap">Try the pool whose sealer runs inside a TEE →</a>`;
  } else {
    el.textContent = '';
  }
}

// ---------- signer UI ----------
function showSigner() {
  $('signer-connected').classList.toggle('hidden', !signer);
  $('signer-setup').classList.toggle('hidden', !!signer);
  const pill = $('signer-pill');
  if (signer) {
    $('signer-account').textContent = signer.accountId;
    $('signer-kind').textContent = signer.kind === 'devkey' ? 'dev key' : 'NEAR wallet';
    pill.innerHTML = `<span class="dot"></span>${signer.accountId.length > 28 ? signer.accountId.slice(0, 14) + '…' + signer.accountId.slice(-8) : signer.accountId}`;
  } else {
    pill.textContent = 'choose who pays gas';
  }
  setRoute(route);
  void refreshLight();
}

async function setupSigner() {
  const saved = local.get('signer');
  if (saved) {
    const cfg = JSON.parse(saved);
    if (cfg.kind === 'devkey') signer = new DevKeySigner(cfg.account, cfg.key, RPC_URLS);
  }
  try {
    selectorBundle = await WalletSigner.setup(deployment.pool);
    const fromWallet = WalletSigner.fromSelector(selectorBundle.selector);
    if (fromWallet && (!signer || signer.kind === 'wallet')) signer = fromWallet;
    selectorBundle.selector.store.observable.subscribe(() => {
      const w = WalletSigner.fromSelector(selectorBundle!.selector);
      if (w) { signer = w; local.del('signer'); showSigner(); ($('signer-dialog') as HTMLDialogElement).close(); }
    });
  } catch (e) {
    console.warn('wallet selector unavailable', e);
  }
  showSigner();
}

// ---------- return from a wallet redirect ----------
async function handleRedirectReturn() {
  const params = new URLSearchParams(location.search);
  const hashes = params.get('transactionHashes');
  const err = params.get('errorCode');
  const pendingRaw = local.get('pending');
  if (!hashes && !err) return;
  history.replaceState(null, '', location.pathname + location.hash);
  const pend = pendingRaw ? JSON.parse(pendingRaw) : null;
  local.del('pending');
  if (err) { log(`wallet returned an error: ${err} ${params.get('errorMessage') ?? ''}`, { error: true }); return; }
  const hash = hashes!.split(',')[0];
  try {
    const outcome: any = signer ? await chain.txStatus(hash, signer.accountId) : null;
    const failed = outcome?.receipts_outcome?.some((r: any) => r.outcome?.status?.Failure);
    if (failed) { log(`${pend?.describe ?? 'transaction'}: failed on-chain`, { hash, error: true }); return; }
    log(`${pend?.describe ?? 'transaction'}: confirmed`, { hash, pub: pend?.prepared?.public_view });
  } catch {
    log(`${pend?.describe ?? 'transaction'}: sent`, { hash });
  }
  if (pend?.prepared) saveWallet(wasm.apply_prepared(walletJson!, JSON.stringify(pend.prepared)));
}

// ---------- boot ----------
async function main() {
  setMode((local.get('mode') as 'calm' | 'geek') ?? 'calm');
  $('mode-toggle').addEventListener('change', (e) => setMode((e.target as HTMLInputElement).checked ? 'geek' : 'calm'));
  setPage(pageFromHash());
  window.addEventListener('hashchange', () => setPage(pageFromHash()));

  await init();
  deployment = await loadDeployment();
  chain = new Chain(RPC_URLS, deployment.pool);
  ($('pool-link') as HTMLAnchorElement).href = `${EXPLORER}/address/${deployment.pool}`;
  $('pool-id').textContent = deployment.pool;
  $('pool-id').title = deployment.pool;
  ensureWallet();
  await adoptPool();
  await loadTokens();
  for (const id of ['move-token', 'swap-sell']) {
    const sel = $(id) as HTMLSelectElement;
    for (const t of tokens) sel.add(new Option(t.symbol, String(t.assetId)));
  }
  renderRealms();

  document.querySelectorAll<HTMLButtonElement>('.routes button').forEach((b) => b.addEventListener('click', () => setRoute(b.dataset.route as Route)));
  $('move-amount').addEventListener('input', updateMoveButton);
  $('move-token').addEventListener('change', updateMoveButton);
  $('swap-sell').addEventListener('change', () => { updateSwapBuy(); scheduleRouteQuote(); });
  $('swap-amount').addEventListener('input', scheduleRouteQuote);
  const submit = (formId: string, handler: (btn: HTMLButtonElement) => Promise<void>) =>
    $(formId).addEventListener('submit', (e) => { e.preventDefault(); void handler((e.target as HTMLFormElement).querySelector('button[type=submit]')!); });
  submit('form-move', doMove);
  submit('form-swap', doSwap);
  $('end-epoch').addEventListener('click', () => doEndEpoch($('end-epoch') as HTMLButtonElement));
  $('sync').addEventListener('click', () => sync());
  const copy = () => { navigator.clipboard.writeText($('address').textContent ?? ''); setToast('Address copied.'); setTimeout(() => setToast(''), 2500); };
  $('copy-address').addEventListener('click', copy);
  $('address-short').addEventListener('click', copy);
  $('export-wallet').addEventListener('click', () => {
    const blob = new Blob([walletJson!], { type: 'application/json' });
    const a = Object.assign(document.createElement('a'), { href: URL.createObjectURL(blob), download: 'antumbra-wallet.json' });
    a.click();
  });
  $('import-wallet').addEventListener('change', async (e) => {
    const f = (e.target as HTMLInputElement).files?.[0];
    if (!f) return;
    saveWallet(await f.text());
    log('wallet imported');
    local.del('pool');
    await adoptPool();
    await sync();
  });
  $('reset-wallet').addEventListener('click', () => {
    if (!confirm('Discard this wallet and its notes?')) return;
    saveWallet(wasm.wallet_new(seed()));
    batchCleared.clear();
    log('new wallet created');
    renderWallet();
  });
  const dialog = $('signer-dialog') as HTMLDialogElement;
  $('signer-pill').addEventListener('click', () => dialog.showModal());
  $('dev-connect').addEventListener('click', () => {
    const account = ($('dev-account') as HTMLInputElement).value.trim();
    const key = ($('dev-key') as HTMLInputElement).value.trim();
    if (!account || !key) return fail(new Error('account id and secret key are required'));
    signer = new DevKeySigner(account, key, RPC_URLS);
    local.set('signer', JSON.stringify({ kind: 'devkey', account, key }));
    showSigner();
    dialog.close();
    log(`signing with the dev key of ${account}`);
  });
  $('dev-create').addEventListener('click', async (e) => {
    const btn = e.currentTarget as HTMLButtonElement;
    await busy(btn, async () => {
      btn.querySelector('strong')!.textContent = 'Creating…';
      const { accountId, secretKey } = await createDevAccount();
      signer = new DevKeySigner(accountId, secretKey, RPC_URLS);
      local.set('signer', JSON.stringify({ kind: 'devkey', account: accountId, key: secretKey }));
      showSigner();
      dialog.close();
      log(`created the funded testnet account ${accountId}; its key stays in this browser`);
    });
    btn.querySelector('strong')!.textContent = 'Create a throwaway testnet account';
  });
  $('connect-wallet').addEventListener('click', () => selectorBundle ? (dialog.close(), selectorBundle.modal.show()) : fail(new Error('wallet selector unavailable')));
  $('signer-disconnect').addEventListener('click', async () => {
    await signer?.disconnect().catch(() => undefined);
    signer = null;
    local.del('signer');
    showSigner();
  });

  setRoute('in');
  await setupSigner();
  await refreshPool();
  await handleRedirectReturn();
  await sync(false);
  if (!signer) setTimeout(() => dialog.showModal(), 600);
  setInterval(refreshPool, 12000);
  void ensureKeys().catch(fail);
}

main().catch(fail);
