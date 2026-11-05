import './style.css';
import init, * as wasm from './wasm/antumbra_wasm.js';
import { Chain, loadDeployment, type Deployment, type EpochInfo, type PairInfo } from './chain';
import { DevKeySigner, WalletSigner, createDevAccount, type Signer, type TxResult } from './signer';
import { getProvingKey, local, putProvingKey } from './storage';

/// Public testnet RPCs, tried in order; `?rpc=<url>` puts your own first. Each one limits requests per IP.
const rpcParam = new URLSearchParams(location.search).get('rpc');
const RPC_URLS = [...(rpcParam ? [rpcParam] : []), 'https://test.rpc.fastnear.com', 'https://near-testnet.drpc.org', 'https://rpc.testnet.near.org'];
const EXPLORER = 'https://testnet.nearblocks.io';
const GAS_POOL = 200;
const GAS_FT = 100;
const EPOCH_BLOCKS = 20;
/// NEP-145 registration deposit; a token refunds whatever exceeds its minimum.
const STORAGE_DEPOSIT = '2500000000000000000000';
const ONE_NEAR = '1000000000000000000000000';

type Route = 'in' | 'out' | 'send';
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

// ---------- mode ----------
function setMode(mode: 'calm' | 'geek') {
  document.body.dataset.mode = mode;
  ($('mode-toggle') as HTMLInputElement).checked = mode === 'geek';
  local.set('mode', mode);
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

let lastRender = '';
let lastBalances = '';
function renderWallet() {
  if (!walletJson) return;
  const summaryJson = wasm.wallet_summary(walletJson);
  const key = [summaryJson, epochInfo?.epoch, epochInfo?.blocks_left, pair?.pair_id, claiming, signer?.kind, [...batchCleared.entries()].join(','), tokens.map((t) => t.symbol).join()].join('|');
  if (key === lastRender) return;
  lastRender = key;
  const s = JSON.parse(summaryJson);
  $('address').textContent = s.address;
  $('address-short').textContent = shortAddress(s.address);

  // Balances re-render (and unveil again) only when they change.
  const balancesKey = JSON.stringify(s.balances) + tokens.map((t) => t.symbol).join();
  if (balancesKey !== lastBalances) {
    lastBalances = balancesKey;
    const b = $('balances');
    b.innerHTML = '';
    if (s.balances.length === 0) b.innerHTML = '<div class="balance empty"><span class="amount">nothing in the shade yet</span></div>';
    for (const x of s.balances) {
      const t = token(x.asset_id);
      const row = document.createElement('div');
      row.className = 'balance';
      row.innerHTML = `<span class="amount">${fromBase(x.amount, t.decimals)}</span><span class="asset">${t.symbol}</span>`;
      b.appendChild(row);
    }
  }

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
    else if (!batchCleared.get(x.epoch)) { chip = '<span class="chip hot">sealed</span>'; note = 'epoch over, the sealer is clearing it'; }
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

// ---------- the light: what the gas payer holds in public ----------
let lastLight = '';
function pubRow(amount: string, symbol: string, act: { label: string; run: () => Promise<void> } | null): HTMLElement {
  const row = document.createElement('div');
  row.className = 'pub-row';
  row.innerHTML = `<span class="pub-amount">${amount}</span><span class="pub-symbol">${symbol}</span><span class="spacer"></span>`;
  if (act) {
    const btn = document.createElement('button');
    btn.className = 'ghost tiny';
    btn.textContent = act.label;
    btn.addEventListener('click', () => void busy(btn, act.run));
    row.appendChild(btn);
  }
  return row;
}

async function refreshLight() {
  const title = $('light-account');
  const el = $('light-balances');
  if (!signer) {
    if (lastLight !== 'none') {
      lastLight = 'none';
      title.textContent = 'nobody is paying gas yet';
      el.innerHTML = '<p class="muted small">Choose who pays gas (top right) and this side fills in. Everything here is public on NEAR.</p>';
    }
    return;
  }
  const [near, ...bals] = await Promise.all([
    chain.nearBalance(signer.accountId).catch(() => '0'),
    ...tokens.map((t) => chain.ftBalance(t.account, signer!.accountId).catch(() => '0')),
  ]);
  const key = [signer.accountId, near, ...bals].join('|');
  if (key === lastLight) return;
  lastLight = key;
  title.textContent = signer.accountId;
  el.innerHTML = '';
  el.appendChild(pubRow(fromBase(near, 24, 2), 'NEAR', null));
  tokens.forEach((t, i) => {
    const act = t.wrapsNear ? { label: 'wrap 1 NEAR', run: () => wrapNear(t) } : { label: `get 100 ${t.symbol}`, run: () => mintTest(t) };
    el.appendChild(pubRow(fromBase(bals[i], t.decimals), t.symbol, act));
  });
}

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
    }
    await refreshBatches();
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

/// A mote of value crossing the seam between the realms.
function crossing(dir: 'in' | 'out') {
  const el = $('crossing');
  el.classList.remove('go-in', 'go-out');
  void el.offsetWidth;
  el.classList.add(dir === 'in' ? 'go-in' : 'go-out');
}

// ---------- the threshold: into, out of, or within the shade ----------
const ROUTES: Record<Route, { hint: string; verb: string; to: string | null }> = {
  in: { hint: 'Public: the amount and a note commitment. <em>Hidden: who owns the note.</em>', verb: 'Shield', to: null },
  out: { hint: 'Public: the amount and the receiving account. <em>Hidden: which note paid.</em>', verb: 'Unshield', to: 'To NEAR account' },
  send: { hint: 'Public: two nullifiers, two commitments. <em>Hidden: sender, recipient, amount.</em>', verb: 'Send in the shade', to: 'To shielded address' },
};

function setRoute(r: Route) {
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

/// Asks OutLayer to clear the last finished epoch inside a TEE (only when the pool is configured for it).
async function doEndEpoch(btn: HTMLButtonElement) {
  await busy(btn, async () => {
    const s = requireSigner();
    const sealer = await chain.getSealer();
    if (!sealer?.outlayer) throw new Error('this pool clears epochs through its signed sealer; nothing to trigger from here');
    const e = (await chain.currentEpoch()).epoch - 1;
    setToast('Asking the TEE to clear the last epoch…', true);
    const r = await s.call(deployment.pool, 'end_epoch', { epoch: e, pair_id: deployment.pair_id }, 150, '100000000000000000000000');
    if (r === 'redirect') return;
    reportTx(`epoch ${e} sent to the sealer`, r);
    await refreshPool();
  });
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
  history.replaceState(null, '', location.pathname);
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

  await init();
  deployment = await loadDeployment();
  chain = new Chain(RPC_URLS, deployment.pool);
  ($('pool-link') as HTMLAnchorElement).href = `${EXPLORER}/address/${deployment.pool}`;
  $('pool-id').textContent = deployment.pool;
  $('pool-id').title = deployment.pool;
  ensureWallet();
  await loadTokens();
  for (const id of ['move-token', 'swap-sell']) {
    const sel = $(id) as HTMLSelectElement;
    for (const t of tokens) sel.add(new Option(t.symbol, String(t.assetId)));
  }

  document.querySelectorAll<HTMLButtonElement>('.routes button').forEach((b) => b.addEventListener('click', () => setRoute(b.dataset.route as Route)));
  $('move-amount').addEventListener('input', updateMoveButton);
  $('move-token').addEventListener('change', updateMoveButton);
  $('swap-sell').addEventListener('change', updateSwapBuy);
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
