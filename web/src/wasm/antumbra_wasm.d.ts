/* tslint:disable */
/* eslint-disable */

export class Prover {
    free(): void;
    [Symbol.dispose](): void;
    constructor(joinsplit: Uint8Array | number[], swap: Uint8Array | number[], swapclaim: Uint8Array | number[]);
    /**
     * Prepared `swap_claim` call for swap `index`. `batch_json` is the contract's
     * `get_batch` view. Returns `{ prepared, outputs: [{asset_id, amount}, ...] }`.
     */
    prepare_claim(wallet_json: string, index: number, asset_1: number, asset_2: number, batch_json: string, seed_bytes: Uint8Array | number[]): string;
    /**
     * Prepared `swap` call. `epoch` is the contract's current epoch.
     */
    prepare_swap(wallet_json: string, pair_id: number, asset_1: number, asset_2: number, sell_asset: number, amount_str: string, epoch: number, seed_bytes: Uint8Array | number[]): string;
    /**
     * Prepared `joinsplit` call for a private transfer.
     */
    prepare_transfer(wallet_json: string, asset_id: number, amount_str: string, to_address: string, seed_bytes: Uint8Array | number[]): string;
    /**
     * Prepared `joinsplit` call for a withdrawal to a NEAR account.
     */
    prepare_withdraw(wallet_json: string, asset_id: number, amount_str: string, to_account: string, seed_bytes: Uint8Array | number[]): string;
}

/**
 * Applies a prepared call's bookkeeping once its transaction succeeded. Returns the wallet.
 */
export function apply_prepared(wallet_json: string, prepared_json: string): string;

/**
 * Marks notes/swaps (by index) spent/claimed after the chain confirmed it. Returns the wallet.
 */
export function apply_spent(wallet_json: string, note_idxs_json: string, swap_idxs_json: string): string;

/**
 * Runs a circuit's deterministic setup; returns the serialised proving key (cache it).
 */
export function generate_proving_key(name: string): Uint8Array;

/**
 * `{ notes: [[index, nullifier]], swaps: [[index, nullifier]] }` for everything unspent/unclaimed.
 */
export function nullifiers(wallet_json: string): string;

/**
 * Deposit message for `ft_transfer_call`. Returns `{ msg, commitment }`.
 */
export function prepare_deposit(wallet_json: string, asset_id: number, amount_str: string, seed_bytes: Uint8Array | number[]): string;

/**
 * Appends leaves (`[{position, cm, ciphertext}]`, base64 ciphertexts) and trial-decrypts them.
 * Returns `{ wallet, report }`.
 */
export function scan(wallet_json: string, leaves_json: string): string;

export function start(): void;

export function wallet_address(wallet_json: string): string;

/**
 * New wallet from 32 random bytes. Returns the wallet JSON.
 */
export function wallet_new(seed_bytes: Uint8Array | number[]): string;

/**
 * Public view of the wallet for the UI: balances, notes and swaps without secrets.
 */
export function wallet_summary(wallet_json: string): string;

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly __wbg_prover_free: (a: number, b: number) => void;
    readonly apply_prepared: (a: number, b: number, c: number, d: number, e: number) => void;
    readonly apply_spent: (a: number, b: number, c: number, d: number, e: number, f: number, g: number) => void;
    readonly generate_proving_key: (a: number, b: number, c: number) => void;
    readonly nullifiers: (a: number, b: number, c: number) => void;
    readonly prepare_deposit: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number) => void;
    readonly prover_new: (a: number, b: number, c: number, d: number, e: number, f: number, g: number) => void;
    readonly prover_prepare_claim: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number, i: number, j: number, k: number) => void;
    readonly prover_prepare_swap: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number, i: number, j: number, k: number, l: number, m: number) => void;
    readonly prover_prepare_transfer: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number, i: number, j: number, k: number) => void;
    readonly prover_prepare_withdraw: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number, i: number, j: number, k: number) => void;
    readonly scan: (a: number, b: number, c: number, d: number, e: number) => void;
    readonly start: () => void;
    readonly wallet_address: (a: number, b: number, c: number) => void;
    readonly wallet_new: (a: number, b: number, c: number) => void;
    readonly wallet_summary: (a: number, b: number, c: number) => void;
    readonly __wbindgen_export: (a: number, b: number, c: number) => void;
    readonly __wbindgen_export2: (a: number, b: number) => number;
    readonly __wbindgen_export3: (a: number, b: number, c: number, d: number) => number;
    readonly __wbindgen_externrefs: WebAssembly.Table;
    readonly __wbindgen_add_to_stack_pointer: (a: number) => number;
    readonly __wbindgen_export4: (a: number) => void;
    readonly __wbindgen_start: () => void;
}

export type SyncInitInput = BufferSource | WebAssembly.Module;

/**
 * Instantiates the given `module`, which can either be bytes or
 * a precompiled `WebAssembly.Module`.
 *
 * @param {{ module: SyncInitInput }} module - Passing `SyncInitInput` directly is deprecated.
 *
 * @returns {InitOutput}
 */
export function initSync(module: { module: SyncInitInput } | SyncInitInput): InitOutput;

/**
 * If `module_or_path` is {RequestInfo} or {URL}, makes a request and
 * for everything else, calls `WebAssembly.instantiate` directly.
 *
 * @param {{ module_or_path: InitInput | Promise<InitInput> }} module_or_path - Passing `InitInput` directly is deprecated.
 *
 * @returns {Promise<InitOutput>}
 */
export default function __wbg_init (module_or_path?: { module_or_path: InitInput | Promise<InitInput> } | InitInput | Promise<InitInput>): Promise<InitOutput>;
