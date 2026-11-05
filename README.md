# Antumbra

A shielded pool with batch swaps on NEAR: inputs are burned from a shielded
pool (unlinkable to the sender), every swap in an epoch clears at one price,
outputs are minted privately. Groth16 over
BLS12-381, verified on-chain through NEAR's NEP-488 host functions.

Built for the [NEAR HACKATHON](https://nearhackathon.com/) (1–14 November 2026).
Tracks: `privacy`, `defi`, `open`.

## Status

| Day | Delivered |
| --- | --- |
| 1 | Gas spike: which curve and verification path to use on NEAR (results below) |
| 2 | Shielded pool: public deposits of NEP-141 tokens into notes, private join-split transfers, withdrawals to NEAR accounts, note delivery by encrypted ciphertexts and wallet scanning. End-to-end test on a local sandbox |
| 3 | Swaps: a shielded swap action, per-epoch batches executed against a constant-product pool at one clearing price, a SwapClaim circuit that mints the pro-rata output privately. End-to-end swap cycle on the sandbox |
| 4 | `antumbra` CLI wallet (keys, scanning, deposit, transfer, withdraw, swap, claim) with a one-command deployer; the whole flow runs on NEAR testnet (see below) |
| 5 | Attack cases in the sandbox tests, `scripts/check.sh`, demo script |
| 6 | Browser wallet (`web/`): proving in WebAssembly inside a Web Worker (about 3 s per proof), notes scanned and kept in the browser, signing through a NEAR wallet or a throwaway funded testnet account created in-page |

Protocol details: [docs/DESIGN.md](docs/DESIGN.md).

## Layout

| Path | What |
| --- | --- |
| `crates/antumbra-core` | Poseidon (BLS12-381 Fr), notes, nullifiers, commitment tree, note encryption, R1CS gadgets. Shared by contract, circuits and wallet |
| `crates/antumbra-circuits` | Groth16 circuits (join-split, swap, swap claim), deterministic setup, NEAR byte encodings, `antumbra-setup` binary |
| `contracts/pool` | The shielded pool contract (`antumbra_pool.wasm`) |
| `contracts/test-ft` | Minimal NEP-141 token with an open `mint`, for tests |
| `crates/antumbra-wallet` | Wallet library: local state, chain client, proving-key cache, the user-level operations |
| `crates/antumbra-cli` | The `antumbra` command-line wallet and deployer |
| `crates/antumbra-wasm` | wasm-bindgen bindings of the wallet for the browser |
| `web` | The browser wallet (Vite + TypeScript, near-api-js, wallet-selector) |
| `tests/e2e` | Sandbox tests: the transfer cycle (deposit, private transfer, scan, withdraw, two rejected attacks) and the swap cycle (liquidity, swap, epoch execution, claim, withdrawal of the bought asset) |
| `scripts/demo.sh` | Runs the whole flow through the CLI against a deployment |
| `keys` | Embedded verifying keys (regenerable, byte-for-byte) |
| `spike` | Day-1 gas measurements of five verification paths |

## Try it

Requirements: Rust 1.99 with the `wasm32-unknown-unknown` target (pinned by
`rust-toolchain.toml`), network access on first run (the NEAR sandbox binary,
nearcore 2.14.0, is downloaded by `near-workspaces`).

One command runs everything below in order: `scripts/check.sh`. Expect the
three constraint counts, `all checks passed`, `swap cycle passed` and
`test result: ok` lines, in about ten minutes on first run (dependencies and
the sandbox download) and about two minutes after that.

1. `cargo test -p antumbra-core --all-features -p antumbra-circuits --release -- --nocapture`
   Unit tests: gadget-vs-native hash and tree consistency, note encryption,
   AMM arithmetic, and the three circuits (prints `joinsplit constraints:
   17376`, `swap constraints: 17625`, `swapclaim constraints: 22349`), plus a
   native prove-and-verify. Expect all tests to pass.
2. `scripts/build-contracts.sh`
   Regenerates `keys/*.vk.near` (prints their sha256; identical on every
   machine) and builds both contracts to `target/wasm32-unknown-unknown/release/`.
3. `CARGO_TARGET_DIR=target/tests cargo test -p antumbra-e2e -- --nocapture`
   Starts local sandboxes and runs two scenarios, each printing a gas row per call.
   `deposit_transfer_scan_withdraw`: Alice deposits 600 000 units into a note;
   Alice privately pays Bob 250 000 (join-split with one real and one dummy
   input); Bob scans the on-chain ciphertext log and finds exactly his note;
   Bob withdraws 100 000 to his NEAR account; replaying the spent note fails
   with `note already spent`; a withdrawal proof bound to Bob cannot pay Alice
   (`invalid proof`). Expect `all checks passed`.
   `swap_cycle`: Alice seeds a pool with 1 000 000 of each asset, deposits
   600 000 of asset 1, swaps 100 000 of it in the current epoch (change stays
   private), the epoch ends and anyone executes the batch (`lambda_2 = 90661`,
   the constant-product output after the 0.3% fee), Alice finds her swap by
   scanning, claims 90 661 of asset 2 as a private note, and withdraws it to
   her account. Along the way the contract rejects a swap that sells the wrong
   side of the pair, a swap aimed at a future epoch, a claim before the epoch
   has ended, a replayed claim (`note already spent`) and a valid swap aimed
   at an already executed batch (`batch already executed`, with the call
   reverted entirely). Expect `swap cycle passed` and `test result: ok` for both.

The `spike` crates are independent: see [spike results](#spike-results-day-1).

## Try it in the browser

Hosted: **https://valentine1898.github.io/antumbra/** (built from `web/` by
the Pages workflow on every push). Or run it locally:

```
cd web && npm install && npm run dev
```

Open http://localhost:5173. Either way the page talks to the testnet
deployment in `deployment.json` through FastNEAR's public RPC.

1. **Gas payer.** Connect MyNearWallet, or expand "testnet dev key" and press
   *Create a funded dev account*: a key pair is generated in the browser and
   the testnet helper service funds a fresh `dev-…testnet` account. Press
   *Mint 1 000 000 test tokens* to get both test tokens.
2. **Deposit** 600 000 of token 1. The log shows what went on-chain: the
   amount and a note commitment. *Sync* finds the note by trial decryption and
   the shielded balance appears.
3. **Transfer** 250 000 to a shielded address (your own, from the wallet card,
   works as a demo). The proof is built in a Web Worker in about three
   seconds; the log shows the two nullifiers and two commitments the chain saw.
4. **Withdraw** 100 000 to any NEAR account; **Swap** 100 000 of asset 1 in
   the current epoch; once the epoch (20 blocks) has ended, **Claim** executes
   the batch if nobody has, proves your share and mints it as a private note.
   *Sync* again and the asset-2 balance shows up; withdraw it if you like.

Proving keys are regenerated from the seeded setups on first use (about half
a minute per circuit, once) and cached in IndexedDB. The wallet (keys and
notes) lives in localStorage; export it from *Backup / restore* before
clearing site data. `npm run build` produces a static site in `web/dist`.
The wasm bindings in `web/src/wasm` are committed; after changing the Rust
crates rebuild them with `scripts/build-wasm.sh` (needs `wasm-pack`).

## Try it on testnet with the CLI

The pool and two test tokens are deployed on NEAR testnet; `deployment.json`
in the repository root points at them. Build the CLI and run the demo:

1. `cargo build -p antumbra-cli --release`
2. `ANTUMBRA_RPC=https://test.rpc.fastnear.com scripts/demo.sh deployment.json`
   This needs a funded testnet account to pay gas: put its id and key in
   `deployment.secret.json` (`{"deployer_account": "...", "deployer_key":
   "ed25519:...", "pool_key": ""}`) or export `ANTUMBRA_ACCOUNT` and
   `ANTUMBRA_KEY`. The script creates two wallets, deposits, transfers
   privately, withdraws, swaps, waits for the epoch, claims and withdraws the
   bought asset, printing gas and the pool state at each step. Test tokens
   can be minted by anyone with `antumbra mint`.

Or deploy your own copy and run against it:

```
antumbra --network testnet deploy      # dev accounts through the testnet helper, writes deployment.json
antumbra --wallet alice.json keys-new  # prints a shielded address antumbra:<pk>:<enc>
antumbra --wallet alice.json deposit --token <token id> --amount 600000
antumbra --wallet alice.json sync
antumbra --wallet alice.json transfer --asset 1 --amount 250000 --to antumbra:...
antumbra --wallet alice.json swap --pair 1 --sell 1 --amount 100000
antumbra --wallet alice.json claim    # once the epoch (20 blocks) has ended; executes the batch if needed
antumbra --wallet alice.json withdraw --asset 2 --amount 90661 --to <near account>
antumbra info
```

`deploy` also works against a local sandbox: pass `--network http://127.0.0.1:3030
--account test.near --key <validator key>` and the contracts go to fresh
subaccounts. The NEAR account that signs a transaction is visible on-chain as
the gas payer; use a dedicated account (or a relayer) when that link matters.

## How a private transfer works

```
pk  = H(DOM_PK, sk)                       owner key
nk  = H(DOM_NK, sk)                       nullifier key
cm  = H(DOM_NOTE, asset_id, amount, pk, rseed)
nf  = H(DOM_NF, nk, cm, position)
```

A deposit is `ft_transfer_call` with a note commitment and a ciphertext; the
pool inserts the commitment into a Poseidon Merkle tree (depth 20) and stores
the ciphertext. A join-split proves, in zero knowledge, that two input notes
exist in the tree, reveals their nullifiers, commits to two output notes and
balances the amounts (with an optional public amount for deposits and
withdrawals), all bound to one asset and to an external-data hash. The contract
verifies the proof with the BLS12-381 host functions (17 Tgas), records the
nullifiers and appends the new commitments. Recipients find their notes by
trial-decrypting the ciphertext log (X25519 + ChaCha20-Poly1305).

## How a swap works

A swap is a join-split whose second output is a swap commitment
`scm = H(DOM_SWAP, epoch, pair_id, direction, delta, claim_pk, rseed)`; the
burned amount `delta` is public but unlinkable to the sender. All swaps of an epoch (20 blocks) for a pair are summed; when the
epoch ends anyone calls `end_epoch`, which executes the totals against the
pool and publishes `BatchSwapOutputData` (deltas, outputs, unfilled amounts
per direction). The swapper then proves, in zero knowledge, that a swap
commitment in the tree belongs to them and that their outputs are the exact
pro-rata share of the published result, and the contract mints those outputs
as private notes. Only the batch totals and which batch is claimed are
public; the link between swap and claim is not.

## Measured gas (sandbox, nearcore 2.14.0)

| Call | Tgas |
| --- | --- |
| Groth16 verification, BLS12-381 host functions, 8 public inputs | ~17 |
| Poseidon(2) hash in wasm | ~1.5 |
| deposit (`ft_transfer_call` round trip, one tree insert) | 38 total |
| join-split transfer (verify + 2 tree inserts) | 83 |
| join-split withdrawal (same + `ft_transfer`) | 88 total |
| swap (verify + 2 tree inserts) | 86 |
| end_epoch (batch execution, no proof) | 11 |
| swap_claim (verify + 2 tree inserts) | 81 |

The 300 Tgas limit per call leaves room for one proof plus tree updates per
action, which is why the protocol uses one proof per action.

## Security notes

- **Development trusted setup.** `antumbra-setup` seeds the Groth16 setup with
  a fixed value so anyone can regenerate the verifying key the contract embeds.
  The toxic waste is public; a real deployment needs an MPC ceremony.
- **Poseidon without length padding.** `hash` is a fixed-arity primitive; every
  protocol hash is a fixed-length tuple starting with a domain tag, and the
  circuits enforce the same shapes.
- **Dummy inputs.** Every join-split has two inputs; a zero-value dummy skips
  the membership check but still yields a nullifier, which the contract
  records. Dummies therefore use fresh randomness per transaction.
- **Deposits and swap inputs are public amounts.** A deposit's amount and
  commitment are visible; a swap reveals how much is burned, unlinkably.
  Privacy starts inside the pool; sealed swap amounts are a stretch goal.
- **Amounts are 112-bit.** Deposits, swaps and withdrawals above 2^112 - 1
  are rejected, so in-circuit products stay below the field modulus.
- **Claims reveal the batch.** A claim names `(epoch, pair_id)` so the
  contract can look up the batch result, but not which swap it belongs to.
- Not audited. Not for real funds.

## Spike results (day 1)

Toy Groth16 circuit with 4 public inputs, one verifier method per path.
`receipt` is the gas burnt by the function-call receipt, including ~1.7 Tgas
of fixed call cost. Reproduce with the three `spike` steps:
`cargo run -p spike-gen --release`, then
`cargo build -p spike-contract --target wasm32-unknown-unknown --release`, then
`cargo test -p spike-tests -- --nocapture`.

| Path | receipt Tgas |
| --- | --- |
| BN254, `alt_bn128_*` host functions | 34.2 |
| BLS12-381, NEP-488 `bls12381_*` host functions | 17.2 |
| BN254, pure wasm (ark-groth16) | 56.5 |
| BLS12-377, pure wasm (ark-groth16) | 129.9 |

Pure-wasm verification over BLS12-377 fits under the 300 Tgas call limit but
leaves room for only one proof per transaction, which rules out designs with
several proofs per transaction.

## License

MIT, see `LICENSE`.
