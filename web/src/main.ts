import './style.css';
import init, * as wasm from './wasm/antumbra_wasm.js';
import { Chain, loadDeployment, type Deployment, type EpochInfo, type PairInfo } from './chain';
import { DevKeySigner, WalletSigner, createDevAccount, type Signer, type TxResult } from './signer';
import { getProvingKey, local, putProvingKey } from './storage';

const RPC = new URLSearchParams(location.search).get('rpc') ?? 'https://test.rpc.fastnear.com';
const EXPLORER = 'https://testnet.nearblocks.io';
const GAS_POOL = 200;
const GAS_FT = 100;
const EPOCH_BLOCKS = 20;

const $ = <T extends HTMLElement = HTMLElement>(id: string) => document.getElementById(id) as T;

let deployment: Deployment;
let chain: Chain;
let signer: Signer | null = null;
let walletJson: string | null = local.get('wallet');
let epochInfo: EpochInfo | null = null;
let pair: PairInfo | null = null;
let selectorBundle: Awaited<ReturnType<typeof WalletSigner.setup>> | null = null;

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

/// Test tokens carry no metadata, so they are named by their asset id.
function assetName(assetId: number): string {
  return `token ${assetId}`;
}
function assetLabel(assetId: number): string {
  const token = deployment.tokens[assetId - 1];
  return token ? `token ${assetId} · ${token}` : `asset ${assetId}`;
}

let lastRender = '';
function renderWallet() {
  if (!walletJson) return;
  const summaryJson = wasm.wallet_summary(walletJson);
  const key = summaryJson + '|' + (epochInfo?.epoch ?? '') + '|' + (pair ? pair.asset_1 + ':' + pair.asset_2 : '');
  if (key === lastRender) return;
  lastRender = key;
  const s = JSON.parse(summaryJson);
  $('address').textContent = s.address;
  $('address-short').textContent = shortAddress(s.address);

  const b = $('balances');
  b.innerHTML = '';
  if (s.balances.length === 0) {
    b.innerHTML = '<div class="balance empty"><span class="amount">nothing in the shade yet</span></div>';
  }
  for (const x of s.balances) {
    const row = document.createElement('div');
    row.className = 'balance';
    row.innerHTML = `<span class="amount">${Number(x.amount).toLocaleString('en-US')}</span><span class="asset">${assetName(x.asset_id)}</span>`;
    b.appendChild(row);
  }

  const sw = $('swaps');
  sw.innerHTML = '';
  for (const x of s.swaps) {
    const row = document.createElement('div');
    row.className = 'swap-row';
    let chip = '<span class="chip ok">claimed</span>';
    if (!x.claimed) chip = epochInfo && x.epoch < epochInfo.epoch ? '<span class="chip hot">claimable</span>' : '<span class="chip">clearing</span>';
    const side = x.direction === 1 ? pair?.asset_1 ?? 1 : pair?.asset_2 ?? 2;
    row.innerHTML = `${chip}<span>swap of ${Number(x.delta).toLocaleString('en-US')} ${assetName(side)}, epoch ${x.epoch}</span>`;
    sw.appendChild(row);
  }

  const nt = $('notes-table').querySelector('tbody')!;
  nt.innerHTML = s.notes.length ? '' : '<tr class="empty"><td colspan="5">no notes yet</td></tr>';
  for (const n of s.notes) {
    const tr = document.createElement('tr');
    if (n.spent) tr.className = 'spent';
    tr.innerHTML = `<td>${n.index}</td><td>${n.asset_id}</td><td>${n.amount}</td><td>${n.position}</td><td>${n.spent ? 'spent' : 'unspent'}</td>`;
    nt.appendChild(tr);
  }
  const st = $('swaps-table').querySelector('tbody')!;
  st.innerHTML = s.swaps.length ? '' : '<tr class="empty"><td colspan="6">no swaps yet</td></tr>';
  for (const x of s.swaps) {
    const tr = document.createElement('tr');
    tr.innerHTML = `<td>${x.index}</td><td>${x.epoch}</td><td>${x.direction === 1 ? '1→2' : '2→1'}</td><td>${x.delta}</td><td>${x.position}</td><td>${x.claimed ? 'claimed' : 'open'}</td>`;
    st.appendChild(tr);
  }
}

async function sync(verbose = true) {
  ensureWallet();
  $('sync').classList.add('spinning');
  $('sync-status').textContent = 'syncing…';
  try {
    const total = await chain.leafCount();
    let from = JSON.parse(wasm.wallet_summary(walletJson!)).leaves as number;
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

// ---------- pool ----------
async function refreshPool() {
  try {
    const [e, p, root, leaves] = await Promise.all([chain.currentEpoch(), chain.getPair(deployment.pair_id), chain.getRoot(), chain.leafCount()]);
    epochInfo = e;
    pair = p;
    $('epoch').textContent = String(e.epoch);
    $('epoch-left').textContent = `${e.blocks_left} blocks left`;
    ($('epoch-bar') as HTMLElement).style.width = `${Math.round(((EPOCH_BLOCKS - e.blocks_left) / EPOCH_BLOCKS) * 100)}%`;
    $('swap-epoch').textContent = String(e.epoch);
    $('swap-left').textContent = String(e.blocks_left);
    $('pool-root').textContent = root;
    $('pool-root').title = root;
    $('leaves').textContent = String(leaves);
    if (p) $('pair').textContent = `${assetName(p.asset_1)} ${Number(p.reserve_1).toLocaleString('en-US')} / ${assetName(p.asset_2)} ${Number(p.reserve_2).toLocaleString('en-US')}`;
    const batch = await chain.getBatch(e.epoch, deployment.pair_id);
    $('batch').textContent = batch ? `epoch ${e.epoch}: delta_1 ${batch.delta_1}, delta_2 ${batch.delta_2}` : `epoch ${e.epoch}: empty so far`;
    renderWallet();
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

async function sendPrepared(prepared: any, describe: string): Promise<void> {
  const s = requireSigner();
  local.set('pending', JSON.stringify({ prepared, describe }));
  setToast('Waiting for the chain…', true);
  const r = await s.call(deployment.pool, prepared.method, prepared.args, GAS_POOL, '0');
  if (r === 'redirect') return;
  local.del('pending');
  reportTx(describe, r, prepared.public_view);
  if (r.ok) saveWallet(wasm.apply_prepared(walletJson!, JSON.stringify(prepared)));
  await syncSoon();
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

function amountOf(id: string): string {
  const v = ($(id) as HTMLInputElement).value.replace(/[\s,_]/g, '');
  if (!/^\d+$/.test(v) || v === '0') throw new Error('enter a whole positive amount');
  return v;
}

// ---------- actions ----------
async function doDeposit(btn: HTMLButtonElement) {
  await busy(btn, async () => {
    const s = requireSigner();
    const token = ($('deposit-token') as HTMLSelectElement).value;
    const assetId = deployment.tokens.indexOf(token) + 1;
    const amount = amountOf('deposit-amount');
    const d = JSON.parse(wasm.prepare_deposit(walletJson!, assetId, amount, seed()));
    local.set('pending', JSON.stringify({ describe: `deposit of ${amount} ${assetName(assetId)}` }));
    setToast('Waiting for the chain…', true);
    const r = await s.call(token, 'ft_transfer_call', { receiver_id: deployment.pool, amount, msg: d.msg }, GAS_FT, '1');
    if (r === 'redirect') return;
    local.del('pending');
    reportTx(`deposit of ${amount} ${assetName(assetId)}`, r, { amount, commitment: d.commitment });
    await syncSoon();
  });
}

async function proveAndSend(btn: HTMLButtonElement, op: string, params: Record<string, unknown>, describe: string) {
  await busy(btn, async () => {
    requireSigner();
    setToast('Proving in your browser…', true);
    const { result, ms } = await prepare(op, params);
    log(`${op} proof built in ${(ms / 1000).toFixed(1)} s, inside a Web Worker`, { geek: true });
    await sendPrepared(JSON.parse(result), describe);
  });
}

async function doTransfer(btn: HTMLButtonElement) {
  const asset_id = Number(($('transfer-asset') as HTMLSelectElement).value);
  const amount = amountOf('transfer-amount');
  const to = ($('transfer-to') as HTMLInputElement).value.trim();
  await proveAndSend(btn, 'transfer', { asset_id, amount, to }, `private transfer of ${amount} ${assetName(asset_id)}`);
}

async function doWithdraw(btn: HTMLButtonElement) {
  const asset_id = Number(($('withdraw-asset') as HTMLSelectElement).value);
  const amount = amountOf('withdraw-amount');
  const to = ($('withdraw-to') as HTMLInputElement).value.trim();
  await proveAndSend(btn, 'withdraw', { asset_id, amount, to }, `withdrawal of ${amount} ${assetName(asset_id)} to ${to}`);
}

async function doSwap(btn: HTMLButtonElement) {
  if (!pair) return fail(new Error('pair not loaded yet'));
  const sell_asset = Number(($('swap-sell') as HTMLSelectElement).value);
  const amount = amountOf('swap-amount');
  const e = await chain.currentEpoch();
  await proveAndSend(btn, 'swap', { pair_id: pair.pair_id, asset_1: pair.asset_1, asset_2: pair.asset_2, sell_asset, amount, epoch: e.epoch }, `swap of ${amount} ${assetName(sell_asset)} in epoch ${e.epoch}`);
}

async function doEndEpoch(btn: HTMLButtonElement) {
  await busy(btn, async () => {
    const s = requireSigner();
    const e = (await chain.currentEpoch()).epoch - 1;
    setToast('Executing the last epoch…', true);
    const r = await s.call(deployment.pool, 'end_epoch', { epoch: e, pair_id: deployment.pair_id }, 50, '0');
    if (r === 'redirect') return;
    reportTx(`epoch ${e} executed`, r);
    await refreshPool();
  });
}

async function doClaim(btn: HTMLButtonElement) {
  await busy(btn, async () => {
    const s = requireSigner();
    await sync(false);
    const summary = JSON.parse(wasm.wallet_summary(walletJson!));
    const now = await chain.currentEpoch();
    const p = await chain.getPair(deployment.pair_id);
    if (!p) throw new Error('pair not loaded');
    let claimed = 0;
    for (const sw of summary.swaps) {
      if (sw.claimed) continue;
      if (sw.epoch >= now.epoch) { log(`swap #${sw.index} clears when epoch ${sw.epoch} ends (${now.blocks_left} blocks to go)`); continue; }
      let batch = await chain.getBatch(sw.epoch, sw.pair_id);
      if (!batch || !batch.executed) {
        setToast('Executing the batch for everyone…', true);
        const r = await s.call(deployment.pool, 'end_epoch', { epoch: sw.epoch, pair_id: sw.pair_id }, 50, '0');
        if (r === 'redirect') return;
        reportTx(`epoch ${sw.epoch} executed`, r);
        batch = await chain.getBatch(sw.epoch, sw.pair_id);
        if (!batch?.executed) throw new Error('batch still not executed');
      }
      setToast('Proving your share in your browser…', true);
      const { result, ms } = await prepare('claim', { index: sw.index, asset_1: p.asset_1, asset_2: p.asset_2, batch: JSON.stringify(batch) });
      const r = JSON.parse(result);
      log(`claim proof for swap #${sw.index} built in ${(ms / 1000).toFixed(1)} s`, { geek: true });
      await sendPrepared(r.prepared, `claim of ${r.outputs[0].amount} ${assetName(r.outputs[0].asset_id)}${Number(r.outputs[1].amount) ? ` plus a refund of ${r.outputs[1].amount}` : ''}`);
      claimed++;
    }
    if (claimed === 0) { setToast('Nothing to claim yet.'); setTimeout(() => setToast(''), 4000); }
  });
}

async function doMint(btn: HTMLButtonElement) {
  await busy(btn, async () => {
    const s = requireSigner();
    for (const token of deployment.tokens) {
      const r = await s.call(token, 'mint', { account_id: s.accountId, amount: '1000000' }, 30, '0');
      if (r === 'redirect') return;
      reportTx(`minted 1 000 000 ${token.split('.')[0]} to ${s.accountId}`, r);
    }
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
}

async function setupSigner() {
  const saved = local.get('signer');
  if (saved) {
    const cfg = JSON.parse(saved);
    if (cfg.kind === 'devkey') signer = new DevKeySigner(cfg.account, cfg.key, RPC);
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
  chain = new Chain(RPC, deployment.pool);
  ($('pool-link') as HTMLAnchorElement).href = `${EXPLORER}/address/${deployment.pool}`;
  $('pool-id').textContent = deployment.pool;
  ensureWallet();

  const tokenSelect = $('deposit-token') as HTMLSelectElement;
  for (const t of deployment.tokens) tokenSelect.add(new Option(assetLabel(deployment.tokens.indexOf(t) + 1), t));
  for (const id of ['transfer-asset', 'withdraw-asset', 'swap-sell']) {
    const sel = $(id) as HTMLSelectElement;
    deployment.tokens.forEach((_, i) => sel.add(new Option(assetLabel(i + 1), String(i + 1))));
  }

  document.querySelectorAll<HTMLButtonElement>('.tabs button').forEach((b) =>
    b.addEventListener('click', () => {
      document.querySelectorAll('.tabs button').forEach((x) => x.classList.remove('active'));
      b.classList.add('active');
      document.querySelectorAll('.tab').forEach((t) => t.classList.add('hidden'));
      $(`form-${b.dataset.tab}`).classList.remove('hidden');
    }),
  );
  const submit = (formId: string, handler: (btn: HTMLButtonElement) => Promise<void>) =>
    $(formId).addEventListener('submit', (e) => { e.preventDefault(); handler((e.target as HTMLFormElement).querySelector('button[type=submit]')!); });
  submit('form-deposit', doDeposit);
  submit('form-transfer', doTransfer);
  submit('form-withdraw', doWithdraw);
  submit('form-swap', doSwap);
  $('claim-all').addEventListener('click', () => doClaim($('claim-all') as HTMLButtonElement));
  $('end-epoch').addEventListener('click', () => doEndEpoch($('end-epoch') as HTMLButtonElement));
  $('mint').addEventListener('click', () => doMint($('mint') as HTMLButtonElement));
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
    log('new wallet created');
    renderWallet();
  });
  const dialog = $('signer-dialog') as HTMLDialogElement;
  $('signer-pill').addEventListener('click', () => dialog.showModal());
  $('dev-connect').addEventListener('click', () => {
    const account = ($('dev-account') as HTMLInputElement).value.trim();
    const key = ($('dev-key') as HTMLInputElement).value.trim();
    if (!account || !key) return fail(new Error('account id and secret key are required'));
    signer = new DevKeySigner(account, key, RPC);
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
      signer = new DevKeySigner(accountId, secretKey, RPC);
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

  await setupSigner();
  await refreshPool();
  await handleRedirectReturn();
  await sync(false);
  if (!signer) setTimeout(() => dialog.showModal(), 600);
  setInterval(refreshPool, 6000);
  void ensureKeys().catch(fail);
}

main().catch(fail);
