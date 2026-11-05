# Antumbra

A shielded pool with batch swaps on NEAR, in the spirit of Penumbra's ZSwap:
inputs are burned from a shielded pool (unlinkable to the sender), every swap
in an epoch clears at one price, outputs are minted privately. Groth16 over
BLS12-381, verified on-chain through NEAR's NEP-488 host functions.

Built for the [NEAR HACKATHON](https://nearhackathon.com/) (1–14 November 2026).
Tracks: `privacy`, `defi`, `open`.

## Status

| Day | Delivered |
| --- | --- |
| 1 | Gas spike: which curve and verification path to use on NEAR (results below) |
| 2 | Shielded pool: public deposits of NEP-141 tokens into notes, private join-split transfers, withdrawals to NEAR accounts, note delivery by encrypted ciphertexts and wallet scanning. End-to-end test on a local sandbox |
| 3 | Swaps: a shielded swap action, per-epoch batches executed against a constant-product pool at one clearing price, a SwapClaim circuit that mints the pro-rata output privately. End-to-end swap cycle on the sandbox |
| next | CLI wallet, testnet deployment, demo |

Protocol details: [docs/DESIGN.md](docs/DESIGN.md).

## Layout

| Path | What |
| --- | --- |
| `crates/antumbra-core` | Poseidon (BLS12-381 Fr), notes, nullifiers, commitment tree, note encryption, R1CS gadgets. Shared by contract, circuits and wallet |
| `crates/antumbra-circuits` | Groth16 circuits (join-split, swap, swap claim), deterministic setup, NEAR byte encodings, `antumbra-setup` binary |
| `contracts/pool` | The shielded pool contract (`antumbra_pool.wasm`) |
| `contracts/test-ft` | Minimal NEP-141 token with an open `mint`, for tests |
| `tests/e2e` | Sandbox tests: the transfer cycle (deposit, private transfer, scan, withdraw, two rejected attacks) and the swap cycle (liquidity, swap, epoch execution, claim, withdrawal of the bought asset) |
| `keys` | Embedded verifying keys (regenerable, byte-for-byte) |
| `spike` | Day-1 gas measurements of five verification paths |

## Try it

Requirements: Rust 1.99 with the `wasm32-unknown-unknown` target (pinned by
`rust-toolchain.toml`), network access on first run (the NEAR sandbox binary,
nearcore 2.14.0, is downloaded by `near-workspaces`).

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
   her account. Expect `swap cycle passed` and `test result: ok` for both.

The `spike` crates are independent: see [spike results](#spike-results-day-1).

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
burned amount `delta` is public (as in Penumbra V1) but unlinkable to the
sender. All swaps of an epoch (20 blocks) for a pair are summed; when the
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
  commitment are visible, as with a transfer into Penumbra; a swap reveals
  how much is burned (unlinkably), as in Penumbra V1. Privacy starts inside
  the pool; sealed swap amounts are a stretch goal.
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

Pure-wasm verification of Penumbra's own curve fits under the 300 Tgas call
limit but leaves room for only one proof per transaction, so Penumbra's
multi-proof transactions cannot be ported as they are.

## License

MIT, see `LICENSE`.
