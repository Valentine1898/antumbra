// Proving runs here so the page stays responsive. Keys are generated from the
// deterministic setups on first use and handed back for caching.
import init, * as wasm from './wasm/antumbra_wasm.js';

const ctx = self as unknown as { onmessage: ((e: MessageEvent) => void) | null; postMessage: (m: unknown, t?: Transferable[]) => void };
const ready = init();
let prover: wasm.Prover | null = null;

ctx.onmessage = async (e: MessageEvent) => {
  const { id, type } = e.data;
  await ready;
  try {
    if (type === 'keys') {
      const have: Record<string, Uint8Array | null> = e.data.keys;
      const bytes: Record<string, Uint8Array> = {};
      for (const name of ['joinsplit', 'swap', 'swapclaim']) {
        if (have[name]) {
          bytes[name] = have[name]!;
        } else {
          ctx.postMessage({ type: 'progress', text: `generating ${name} proving key (one-time, about half a minute)…` });
          const t = performance.now();
          const b = wasm.generate_proving_key(name);
          ctx.postMessage({ type: 'generated', name, bytes: b, ms: Math.round(performance.now() - t) });
          bytes[name] = b;
        }
      }
      prover = new wasm.Prover(bytes.joinsplit, bytes.swap, bytes.swapclaim);
      ctx.postMessage({ id, result: 'ok' });
      return;
    }
    if (type === 'prepare') {
      if (!prover) throw new Error('proving keys not loaded');
      const { op, wallet, p, seed } = e.data;
      const t = performance.now();
      let result: string;
      switch (op) {
        case 'transfer': result = prover.prepare_transfer(wallet, p.asset_id, p.amount, p.to, seed); break;
        case 'withdraw': result = prover.prepare_withdraw(wallet, p.asset_id, p.amount, p.to, seed); break;
        case 'swap': result = prover.prepare_swap(wallet, p.pair_id, p.asset_1, p.asset_2, p.sell_asset, p.amount, p.epoch, seed); break;
        case 'claim': result = prover.prepare_claim(wallet, p.index, p.asset_1, p.asset_2, p.batch, seed); break;
        default: throw new Error(`unknown op ${op}`);
      }
      ctx.postMessage({ id, result, ms: Math.round(performance.now() - t) });
      return;
    }
    throw new Error(`unknown message ${type}`);
  } catch (err) {
    ctx.postMessage({ id, error: String(err instanceof Error ? err.message : err) });
  }
};
