// Read-only view of the pool and tokens through NEAR RPC endpoints.
import { FailoverRpcProvider, JsonRpcProvider } from 'near-api-js';

/// Public testnet RPCs rate-limit per IP; a failover provider moves on to the next one when a call fails.
export function makeProvider(urls: string[]): FailoverRpcProvider {
  return new FailoverRpcProvider(urls.map((url) => new JsonRpcProvider({ url })));
}

export interface Deployment { network: string; pool: string; tokens: string[]; pair_id: number; deployer: string }
export interface EpochInfo { epoch: number; block_height: number; blocks_left: number }
export interface PairInfo { pair_id: number; asset_1: number; asset_2: number; reserve_1: string; reserve_2: string }
export interface BatchInfo {
  epoch: number; pair_id: number; executed: boolean;
  delta_1: string; delta_2: string; lambda_1: string; lambda_2: string; unfilled_1: string; unfilled_2: string;
  bsod_hash: string | null;
  entries: number;
  entries_hash: string;
}
export interface LeafInfo { position: number; cm: string; ciphertext: string }
export interface SealerInfo { pk: [string, string]; executor_pk: string; outlayer: { account: string; wasm_url: string; wasm_hash: string } | null }
/// NEP-148 metadata; test tokens have none.
export interface TokenMeta { symbol: string; name: string; decimals: number; icon: string | null }

export class Chain {
  provider: FailoverRpcProvider;
  rpcUrls: string[];
  pool: string;
  constructor(rpcUrls: string[], pool: string) {
    this.rpcUrls = rpcUrls;
    this.pool = pool;
    this.provider = makeProvider(rpcUrls);
  }
  async view<T>(method: string, args: Record<string, unknown> = {}): Promise<T> {
    return (await this.provider.callFunction({ contractId: this.pool, method, args })) as T;
  }
  async viewOn<T>(contractId: string, method: string, args: Record<string, unknown> = {}): Promise<T> {
    return (await this.provider.callFunction({ contractId, method, args })) as T;
  }
  leafCount() { return this.view<number>('leaf_count'); }
  leaves(from: number, limit: number) { return this.view<LeafInfo[]>('get_leaves', { from, limit }); }
  currentEpoch() { return this.view<EpochInfo>('current_epoch'); }
  getPair(pair_id: number) { return this.view<PairInfo | null>('get_pair', { pair_id }); }
  getBatch(epoch: number, pair_id: number) { return this.view<BatchInfo | null>('get_batch', { epoch, pair_id }); }
  isSpent(nullifier: string) { return this.view<boolean>('is_spent', { nullifier }); }
  getToken(asset_id: number) { return this.view<string | null>('get_token', { asset_id }); }
  getRoot() { return this.view<string>('get_root'); }
  getSealer() { return this.view<SealerInfo | null>('get_sealer'); }
  /// NEP-148 metadata, or null for a token without it. Tries the RPCs in order
  /// itself: a missing method is an answer, not a reason to fail over.
  async ftMetadata(token: string): Promise<TokenMeta | null> {
    for (const url of this.rpcUrls) {
      try {
        return (await new JsonRpcProvider({ url }).callFunction({ contractId: token, method: 'ft_metadata', args: {} })) as TokenMeta;
      } catch (e) {
        if (/method is not found|MethodNotFound/i.test(String(e))) return null;
      }
    }
    return null;
  }
  ftBalance(token: string, account: string) { return this.viewOn<string>(token, 'ft_balance_of', { account_id: account }); }
  /// NEP-145 registration of `account` on `token`, or null when it has none.
  storageBalance(token: string, account: string) { return this.viewOn<{ total: string } | null>(token, 'storage_balance_of', { account_id: account }); }
  async nearBalance(account: string): Promise<string> {
    const a: any = await this.provider.viewAccount({ accountId: account } as any);
    return String(a.amount);
  }
  async txStatus(hash: string, account: string) {
    return this.provider.viewTransactionStatus({ txHash: hash, accountId: account, waitUntil: 'EXECUTED_OPTIMISTIC' } as any);
  }
}

export async function loadDeployment(): Promise<Deployment> {
  const res = await fetch(import.meta.env.BASE_URL + 'deployment.json');
  if (!res.ok) throw new Error('deployment.json missing');
  return res.json();
}
