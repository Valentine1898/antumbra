// Two ways to pay for transactions: a NEAR wallet through wallet-selector, or a
// testnet dev key held in the page (demo only).
import { Account, JsonRpcProvider, KeyPair, KeyPairSigner, actions } from 'near-api-js';
import { setupWalletSelector, type WalletSelector } from '@near-wallet-selector/core';
import { setupMyNearWallet } from '@near-wallet-selector/my-near-wallet';
import { setupModal, type WalletSelectorModal } from '@near-wallet-selector/modal-ui';
import '@near-wallet-selector/modal-ui/styles.css';

export interface TxResult { hash: string; ok: boolean; error?: string; logs: string[] }
export type CallOutcome = TxResult | 'redirect';

export interface Signer {
  kind: 'devkey' | 'wallet';
  accountId: string;
  call(receiverId: string, method: string, args: Record<string, unknown>, gasTgas: number, depositYocto: string): Promise<CallOutcome>;
  disconnect(): Promise<void>;
}

function summarize(outcome: any): TxResult {
  const hash: string = outcome?.transaction?.hash ?? '';
  const logs: string[] = [];
  for (const r of outcome?.receipts_outcome ?? []) for (const l of r.outcome?.logs ?? []) logs.push(l);
  const status = outcome?.status ?? {};
  const failed = status.Failure ?? outcome?.receipts_outcome?.find((r: any) => r.outcome?.status?.Failure)?.outcome?.status?.Failure;
  if (failed) {
    const msg = JSON.stringify(failed);
    const m = msg.match(/Smart contract panicked: ([^"]+)/);
    return { hash, ok: false, error: m ? m[1] : msg, logs };
  }
  return { hash, ok: true, logs };
}

export class DevKeySigner implements Signer {
  kind = 'devkey' as const;
  accountId: string;
  private account: Account;
  constructor(accountId: string, secretKey: string, rpcUrl: string) {
    this.accountId = accountId;
    const provider = new JsonRpcProvider({ url: rpcUrl });
    this.account = new Account(accountId, provider, KeyPairSigner.fromSecretKey(secretKey as any));
  }
  async call(receiverId: string, method: string, args: Record<string, unknown>, gasTgas: number, depositYocto: string): Promise<CallOutcome> {
    const outcome = await this.account.signAndSendTransaction({
      receiverId,
      actions: [actions.functionCall(method, args, BigInt(gasTgas) * 1_000_000_000_000n, BigInt(depositYocto))],
      throwOnFailure: false,
    });
    return summarize(outcome);
  }
  async disconnect() { /* nothing to do */ }
}

export class WalletSigner implements Signer {
  kind = 'wallet' as const;
  accountId: string;
  private selector: WalletSelector;
  private constructor(accountId: string, selector: WalletSelector) {
    this.accountId = accountId;
    this.selector = selector;
  }

  static async setup(contractId: string): Promise<{ selector: WalletSelector; modal: WalletSelectorModal }> {
    const selector = await setupWalletSelector({ network: 'testnet', modules: [setupMyNearWallet()] });
    const modal = setupModal(selector, { contractId });
    return { selector, modal };
  }

  static fromSelector(selector: WalletSelector): WalletSigner | null {
    const accounts = selector.store.getState().accounts;
    const active = accounts.find((a) => a.active) ?? accounts[0];
    return active ? new WalletSigner(active.accountId, selector) : null;
  }

  async call(receiverId: string, method: string, args: Record<string, unknown>, gasTgas: number, depositYocto: string): Promise<CallOutcome> {
    const wallet = await this.selector.wallet();
    const outcome = await wallet.signAndSendTransaction({
      receiverId,
      actions: [{ type: 'FunctionCall', params: { methodName: method, args, gas: (BigInt(gasTgas) * 1_000_000_000_000n).toString(), deposit: depositYocto } }],
      callbackUrl: window.location.origin + window.location.pathname,
    } as any);
    if (!outcome) return 'redirect';
    return summarize(outcome);
  }

  async disconnect() {
    const wallet = await this.selector.wallet();
    await wallet.signOut();
  }
}

/// Creates a funded `dev-…testnet` account through the testnet helper service.
/// The key pair is generated here and never leaves the browser.
export async function createDevAccount(): Promise<{ accountId: string; secretKey: string }> {
  const keyPair = KeyPair.fromRandom('ed25519');
  const stamp = new Date().toISOString().replace(/[-:TZ.]/g, '').slice(0, 14);
  const rand = Array.from(crypto.getRandomValues(new Uint8Array(6))).map((b) => (b % 10).toString()).join('');
  const accountId = `dev-${stamp}-${rand}.testnet`;
  const url = import.meta.env.DEV ? '/helper/account' : 'https://helper.testnet.near.org/account';
  const res = await fetch(url, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ newAccountId: accountId, newAccountPublicKey: keyPair.getPublicKey().toString() }),
  });
  if (!res.ok) throw new Error(`helper service refused: ${res.status} ${await res.text()}`);
  return { accountId, secretKey: keyPair.toString() };
}
