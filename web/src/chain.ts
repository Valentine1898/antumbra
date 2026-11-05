// Read-only view of the pool and tokens through a NEAR RPC endpoint.
import { JsonRpcProvider } from 'near-api-js';

export interface Deployment { network: string; pool: string; tokens: string[]; pair_id: number; deployer: string }
export interface EpochInfo { epoch: number; block_height: number; blocks_left: number }
export interface PairInfo { pair_id: number; asset_1: number; asset_2: number; reserve_1: string; reserve_2: string }
export interface BatchInfo {
  epoch: number; pair_id: number; executed: boolean;
  delta_1: string; delta_2: string; lambda_1: string; lambda_2: string; unfilled_1: string; unfilled_2: string;
  bsod_hash: string | null;
}
export interface LeafInfo { position: number; cm: string; ciphertext: string }

export class Chain {
  provider: JsonRpcProvider;
  rpcUrl: string;
  pool: string;
  constructor(rpcUrl: string, pool: string) {
    this.rpcUrl = rpcUrl;
    this.pool = pool;
    this.provider = new JsonRpcProvider({ url: rpcUrl });
  }
  async view<T>(method: string, args: Record<string, unknown> = {}): Promise<T> {
    return (await this.provider.callFunction({ contractId: this.pool, method, args })) as T;
  }
  leafCount() { return this.view<number>('leaf_count'); }
  leaves(from: number, limit: number) { return this.view<LeafInfo[]>('get_leaves', { from, limit }); }
  currentEpoch() { return this.view<EpochInfo>('current_epoch'); }
  getPair(pair_id: number) { return this.view<PairInfo | null>('get_pair', { pair_id }); }
  getBatch(epoch: number, pair_id: number) { return this.view<BatchInfo | null>('get_batch', { epoch, pair_id }); }
  isSpent(nullifier: string) { return this.view<boolean>('is_spent', { nullifier }); }
  getToken(asset_id: number) { return this.view<string | null>('get_token', { asset_id }); }
  getRoot() { return this.view<string>('get_root'); }
  async ftBalance(token: string, account: string): Promise<string> {
    return (await this.provider.callFunction({ contractId: token, method: 'ft_balance_of', args: { account_id: account } })) as string;
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
