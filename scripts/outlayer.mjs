#!/usr/bin/env node
// Moves a pool's sealer into OutLayer's TEE (Intel TDX), step by step, with
// plain NEAR transactions and one HTTPS call to OutLayer's keystore API.
//
//   node scripts/outlayer.mjs <deployment.json> <step>
//
// Steps, in order (each prints the transaction it sent):
//   upload      publish the sealer program on FastFS; prints its URL and sha256
//   secret      have the keystore generate PROTECTED_SEALER_SEED inside the
//               enclave and store it for this exact program build; nobody ever
//               sees the value
//   pubkey      run the program in the enclave once to learn the sealer's key
//   register    register that key and the OutLayer program on the pool
//   lock        let the secret open only for requests that come from the pool
//   end-epoch <epoch>   clear a finished epoch inside the enclave
//   status      what is registered and stored
//
// Needs: the sealer program (cargo build -p antumbra-sealer --target
// wasm32-wasip1 --release), `npm install` in web/ (for near-api-js), and
// <deployment>.secret.json next to the deployment file, as `antumbra deploy`
// writes it. What the steps learn (URL, hash, public key, transactions) is kept
// in <deployment>.outlayer.json; it holds nothing secret. Testnet only.
import { createRequire } from 'node:module';
import { readFileSync, writeFileSync, existsSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { fileURLToPath } from 'node:url';

const root = fileURLToPath(new URL('..', import.meta.url));
const { Account, JsonRpcProvider, KeyPairSigner, KeyPair, actions } = createRequire(`${root}web/`)('near-api-js');

const [deploymentPath, step, arg] = process.argv.slice(2);
if (!deploymentPath || !step) {
  console.error('usage: node scripts/outlayer.mjs <deployment.json> upload|secret|pubkey|register|lock|end-epoch <epoch>|status');
  process.exit(2);
}
const OUTLAYER = 'outlayer.testnet';
const API = 'https://testnet-api.outlayer.ai';
const PROFILE = 'sealer';
const WASM = `${root}target/wasm32-wasip1/release/antumbra-sealer.wasm`;
// OutLayer's workers skip a request whose limits are left empty, so they are spelled out.
const LIMITS = { max_instructions: 20_000_000_000, max_memory_mb: 128, max_execution_seconds: 60 };
const T = 1_000_000_000_000n, NEAR = 10n ** 24n;

const dep = JSON.parse(readFileSync(deploymentPath, 'utf8'));
const sec = JSON.parse(readFileSync(deploymentPath.replace(/\.json$/, '.secret.json'), 'utf8'));
const statePath = deploymentPath.replace(/\.json$/, '.outlayer.json');
const state = existsSync(statePath) ? JSON.parse(readFileSync(statePath, 'utf8')) : {};
const save = () => writeFileSync(statePath, JSON.stringify(state, null, 2) + '\n');

const provider = new JsonRpcProvider({ url: process.env.ANTUMBRA_RPC ?? 'https://test.rpc.fastnear.com' });
const account = (id, key) => new Account(id, provider, KeyPairSigner.fromSecretKey(key));
const operator = account(sec.deployer_account, sec.deployer_key);
// The pool account owns itself, so its key signs what only the owner may do.
const pool = account(dep.pool, sec.pool_key);
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const within = (ms, p, what) => Promise.race([p, new Promise((_, rej) => setTimeout(() => rej(new Error(`${what}: no answer in ${ms / 1000} s`)), ms))]);
const view = (contractId, method, args = {}) => provider.callFunction({ contractId, method, args });

/// Sends one transaction and follows it until every receipt has run. A call
/// that waits for the enclave takes as long as the enclave does.
async function send(from, receiverId, action) {
  const signed = await within(30_000, from.createSignedTransaction({ receiverId, actions: [action] }), 'signing');
  const sent = await within(60_000, provider.sendJsonRpc('send_tx', { signed_tx_base64: Buffer.from(signed.encode()).toString('base64'), wait_until: 'INCLUDED' }), 'sending');
  const hash = sent.transaction.hash;
  console.log(`  tx ${hash}`);
  for (let i = 0; i < 150; i++) {
    await sleep(2000);
    let r;
    try { r = await within(20_000, provider.sendJsonRpc('tx', { tx_hash: hash, sender_account_id: from.accountId, wait_until: 'NONE' }), 'status'); } catch { continue; }
    if (!['FINAL', 'EXECUTED', 'EXECUTED_OPTIMISTIC'].includes(r.final_execution_status)) continue;
    if (r.status?.SuccessValue === undefined && !r.status?.Failure) continue;
    const logs = r.receipts_outcome.flatMap((o) => o.outcome.logs).filter((l) => !l.startsWith('EVENT_JSON:'));
    for (const l of logs) console.log(`  log: ${l.slice(0, 300)}`);
    if (r.status.Failure) return { hash, ok: false, failure: JSON.stringify(r.status.Failure).slice(0, 400) };
    return { hash, ok: true, value: Buffer.from(r.status.SuccessValue, 'base64').toString('utf8') };
  }
  throw new Error(`transaction ${hash} did not finish in five minutes`);
}
const call = (from, receiverId, method, args, gasTgas, deposit = 0n) => send(from, receiverId, actions.functionCall(method, args, BigInt(gasTgas) * T, deposit));
const need = (key, earlier) => { if (!state[key]) { console.error(`run the "${earlier}" step first`); process.exit(1); } return state[key]; };
const source = () => ({ WasmUrl: { url: need('wasm_url', 'upload'), hash: state.wasm_hash, build_target: 'wasm32-wasip1' } });
const secretsRef = { profile: PROFILE, account_id: dep.pool };
const accessor = () => ({ WasmHash: { hash: need('wasm_hash', 'upload') } });

const u32 = (n) => { const b = Buffer.alloc(4); b.writeUInt32LE(n); return b; };
const str = (s) => Buffer.concat([u32(Buffer.byteLength(s)), Buffer.from(s)]);

if (step === 'upload') {
  // FastFS: the file is the argument of a call that is meant to run out of gas;
  // an indexer serves what the transaction carried over https.
  const bytes = readFileSync(WASM);
  const hash = createHash('sha256').update(bytes).digest('hex');
  const name = `${hash}.wasm`;
  const payload = Buffer.concat([Buffer.from([0]), str(name), Buffer.from([1]), str('application/wasm'), u32(bytes.length), bytes]);
  console.log(`publishing ${bytes.length} bytes, sha256 ${hash}`);
  const r = await send(operator, OUTLAYER, actions.functionCall('__fastdata_fastfs', payload, 1n, 0n));
  Object.assign(state, { wasm_hash: hash, wasm_url: `https://test.fastfs.io/${sec.deployer_account}/${OUTLAYER}/${name}`, upload_tx: r.hash });
  save();
  console.log(`  ${state.wasm_url}\n  (the transaction is expected to fail with "Exceeded the prepaid gas")`);
} else if (step === 'secret') {
  const res = await fetch(`${API}/secrets/add_generated_secret`, {
    method: 'POST', headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ accessor: { type: 'WasmHash', hash: need('wasm_hash', 'upload') }, owner: dep.pool, profile: PROFILE, encrypted_secrets_base64: null, new_secrets: [{ name: 'PROTECTED_SEALER_SEED', generation_type: 'hex32' }] }),
  });
  if (!res.ok) { console.error(`the keystore refused: ${res.status} ${(await res.text()).slice(0, 300)}`); process.exit(1); }
  const { encrypted_data_base64 } = await res.json();
  console.log('generated PROTECTED_SEALER_SEED inside the enclave; storing the ciphertext for this build');
  // For now the secret opens when the pool account itself signs the request: that is how the next step reads the key.
  const r = await call(pool, OUTLAYER, 'store_secrets', { accessor: accessor(), profile: PROFILE, encrypted_secrets_base64: encrypted_data_base64, access: { Whitelist: { accounts: [dep.pool] } }, vault_id: null }, 50, NEAR / 10n);
  if (!r.ok) { console.error(r.failure); process.exit(1); }
  state.secret_tx = r.hash; save();
} else if (step === 'pubkey') {
  const r = await call(pool, OUTLAYER, 'request_execution', { source: source(), resource_limits: LIMITS, input_data: JSON.stringify({ op: 'pubkey' }), secrets_ref: secretsRef, response_format: 'Text' }, 300, NEAR / 50n);
  const answer = r.ok && r.value !== 'null' ? JSON.parse(JSON.parse(r.value)) : null;
  if (!answer?.ok) { console.error(`the enclave did not answer with a key: ${r.failure ?? r.value}`); process.exit(1); }
  Object.assign(state, { pk: answer.pk, pubkey_tx: r.hash }); save();
  console.log(`sealer public key from the enclave: [${answer.pk.join(', ')}]`);
} else if (step === 'register') {
  // The signed-executor path is closed for this pool: the executor key registered here is new, and its secret is dropped.
  const executor = Buffer.from(KeyPair.fromRandom('ed25519').getPublicKey().data).toString('hex');
  const r = await call(pool, dep.pool, 'set_sealer', { pk: need('pk', 'pubkey'), executor_pk: executor, outlayer: { account: OUTLAYER, wasm_url: state.wasm_url, wasm_hash: state.wasm_hash, secrets_profile: PROFILE } }, 30);
  if (!r.ok) { console.error(r.failure); process.exit(1); }
  state.register_tx = r.hash; save();
  console.log('registered on the pool:', JSON.stringify(await view(dep.pool, 'get_sealer')));
} else if (step === 'lock') {
  // From here on the secret opens only for requests made by the pool account, whoever signs the transaction that reaches it.
  const r = await call(pool, OUTLAYER, 'update_access', { accessor: accessor(), profile: PROFILE, new_access: { Predecessor: { condition: { Whitelist: { accounts: [dep.pool] } } } } }, 30, NEAR / 100n);
  if (!r.ok) { console.error(r.failure); process.exit(1); }
  state.lock_tx = r.hash; save();
} else if (step === 'end-epoch') {
  const epoch = Number(arg);
  if (!Number.isInteger(epoch)) { console.error('end-epoch needs the epoch number'); process.exit(2); }
  // The deposit pays OutLayer for the run; what is left over comes back to the pool.
  const r = await call(operator, dep.pool, 'end_epoch', { epoch, pair_id: dep.pair_id }, 300, NEAR / 50n);
  if (!r.ok) { console.error(r.failure); process.exit(1); }
  console.log(`cleared: ${r.value}`);
} else if (step === 'status') {
  console.log('pool sealer:', JSON.stringify(await view(dep.pool, 'get_sealer'), null, 2));
  console.log('secrets stored for the pool:', JSON.stringify(await view(OUTLAYER, 'list_user_secrets', { account_id: dep.pool }), null, 2));
} else {
  console.error(`unknown step "${step}"`);
  process.exit(2);
}
