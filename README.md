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
| 7 | Sealed amounts: the swap circuit proves a verifiable encryption of the amount to the sealer's key, the chain stores only a commitment and a ciphertext, and a sealer (signed executor, or the same program inside OutLayer's TEE) clears each epoch with batch totals |
| 8 | Real tokens and a wallet UI in two pages: the pool on testnet trades wNEAR and USDC.e; a ledger shows every token's lit part (what NEAR sees) and shaded part (what only you see), the Threshold page moves value across, the Swap page trades inside the shade. Batches priced on NEAR Intents (1Click) as a dry run with live mainnet quotes |

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
| `crates/antumbra-sealer` | The sealer: opens sealed amounts and sums a batch; a library plus a WASI program for OutLayer |
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
   600 000 of asset 1, swaps 100 000 of it in the current epoch with the
   amount sealed (the batch shows one entry and no amount), the epoch ends,
   the sealer opens the batch and submits signed totals (`lambda_2 = 90661`,
   the constant-product output after the 0.3% fee), Alice finds her swap by
   scanning, claims 90 661 of asset 2 as a private note, and withdraws it to
   her account. Along the way the contract rejects a swap that sells the wrong
   side of the pair, a swap aimed at a future epoch, a claim before the batch
   is cleared, totals signed by an impostor, wrong totals under the right key,
   a replayed claim (`note already spent`) and a valid swap aimed at an
   already cleared batch (`batch already cleared`, with the call reverted
   entirely). Expect `swap cycle passed` and `test result: ok` for both.

The `spike` crates are independent: see [spike results](#spike-results-day-1).

## Try it in the browser

Hosted: **https://valentine1898.github.io/antumbra/** (built from `web/` by
the Pages workflow on every push). Or run it locally:

```
cd web && npm install && npm run dev
```

Open http://localhost:5173. Either way the page talks to the testnet
deployment in `deployment.json` through public RPC endpoints (FastNEAR first,
then two others; each limits requests per IP, so the page fails over). Add
`?rpc=https://…` to the URL to put your own endpoint first.

The card on the left shows your two realms: **in the light**, the NEAR
account that pays gas and what it holds in public; **in the shade**, your
shielded address and balances, which only you can see, with the share of each
token that is in the shade. Two pages work on them: **Threshold** moves value
into, out of or within the shade; **Swap** trades inside it. A wallet is bound
to the pool it scans; after a redeploy it forgets its notes and rescans (keys
stay). The testnet pool trades real
testnet tokens: `wrap.testnet` (wNEAR) and `usdc.fakes.testnet` (USDC.e, the
test stablecoin NEAR's DEXes use).

1. **Gas payer.** Connect MyNearWallet, or press *Create a throwaway testnet
   account*: a key pair is generated in the browser and the testnet helper
   service funds a fresh `dev-…testnet` account. The light side fills in with
   its NEAR balance; *wrap 1 NEAR* turns some of it into wNEAR and *get 100
   USDC.e* mints from the test faucet.
2. **Threshold → Into the shade.** Shield 0.5 wNEAR: the amount and a note
   commitment go on-chain, nothing says who owns the note. A mote of light
   crosses from the lit block to the shaded one and the shielded balance
   appears there.
3. **Swap page.** Sell 0.1 wNEAR: while you type, the page shows what the
   batch's net leg would fetch on NEAR Intents (a live 1Click quote at mainnet
   prices; nothing moves on testnet). Press *Seal and swap*: the proof is
   built in a Web Worker in about three seconds and the amount goes on-chain
   sealed (Geek mode shows the batch with one entry and no amount). When the
   epoch (20 blocks) ends, the pool's sealer clears it and the swap card turns
   from *sealed* to *cleared*; with a dev key the page claims your share by
   itself, with a wallet press *Claim*. The bought USDC.e appears in the shade.
4. **Threshold → Out of the shade** sends any amount back to a NEAR account
   (yours by default); **To another shade** is a private transfer to a
   shielded address. Every step lists in *Activity* what the chain saw.

Proving keys are regenerated from the seeded setups on first use (about half
a minute per circuit, once) and cached in IndexedDB. The wallet (keys and
notes) lives in localStorage; export it from *Backup / restore* before
clearing site data. `npm run build` produces a static site in `web/dist`.
The wasm bindings in `web/src/wasm` are committed; after changing the Rust
crates rebuild them with `scripts/build-wasm.sh` (needs `wasm-pack`).

## Try it on testnet with the CLI

The pool on NEAR testnet trades wNEAR (`wrap.testnet`) and USDC.e
(`usdc.fakes.testnet`); `deployment.json` in the repository root points at it.
CLI amounts are base units (yoctoNEAR for wNEAR, millionths for USDC.e).
Build the CLI and run the demo:

1. `cargo build -p antumbra-cli --release`
2. `ANTUMBRA_RPC=https://test.rpc.fastnear.com scripts/demo.sh deployment.json`
   This needs a funded testnet account to pay gas: put its id and key in
   `deployment.secret.json` (`{"deployer_account": "...", "deployer_key":
   "ed25519:...", "pool_key": ""}`) or export `ANTUMBRA_ACCOUNT` and
   `ANTUMBRA_KEY`. The script wraps 0.7 NEAR of that account into wNEAR,
   creates two wallets, deposits, transfers privately, withdraws, swaps,
   waits for the epoch and the sealer, claims and withdraws the bought USDC.e,
   printing gas and the pool state at each step. The wait for the sealer is
   the only slow part: seconds while the pool's operator runs a sealer, up to
   several minutes when only the scheduled workflow is clearing the pool. On a deployment with test
   tokens it mints them instead (`antumbra mint` is open to anyone).

Or deploy your own copy and run against it:

```
antumbra --network testnet deploy      # dev accounts through the testnet helper, writes deployment.json
antumbra --wallet alice.json keys-new  # prints a shielded address antumbra:<pk>:<enc>
antumbra --wallet alice.json deposit --token <token id> --amount <base units>
antumbra --wallet alice.json sync
antumbra --wallet alice.json transfer --asset 1 --amount <base units> --to antumbra:...
antumbra --wallet alice.json swap --pair 1 --sell 1 --amount <base units>   # amount sealed to the pool's sealer
antumbra sealer clear --pair 1        # the pool operator opens finished epochs and submits signed totals
antumbra --wallet alice.json claim    # once the epoch is cleared
antumbra --wallet alice.json withdraw --asset 2 --amount <base units> --to <near account>
antumbra info
antumbra route quote --sell 1 --amount <base units>   # live NEAR Intents quote for a batch's net leg (dry run)
antumbra route plan --pair 1 --epoch <cleared epoch>  # how that batch would have cleared through NEAR Intents
```

`antumbra sealer watch --pair 1` keeps clearing epochs as they end
(`scripts/sealer.sh` runs it for the deployment in the repository root);
`deploy` generates the sealer keys, registers them and writes them to
`deployment.secret.json`, which is ignored by git and must be kept: without
it nobody can clear that pool's swaps. The pool behind the hosted wallet is
also cleared by a scheduled workflow (`.github/workflows/sealer.yml`, every
few minutes, keys in a repository secret), so a swap made there can always
be claimed, a little later than with a sealer running next to it. `deploy --token wrap.testnet
--token usdc.fakes.testnet --liquidity <wNEAR base units> --liquidity <USDC.e
base units>` registers existing NEP-141 tokens instead of deploying test ones
(the deployer must hold the liquidity: `antumbra wrap --amount <yocto>` wraps
NEAR, `antumbra mint` works on the fakes); that is how the testnet pool in
`deployment.json` was made. To clear epochs inside a TEE
instead, build the sealer program (`cargo build -p antumbra-sealer --target
wasm32-wasip1 --release`), publish it at an https URL, give the program a
`PROTECTED_SEALER_SEED` secret in OutLayer, register the same public key
plus `--outlayer-account outlayer.testnet --wasm-url … --wasm-hash …` with
`antumbra sealer register`, and `end_epoch` (with a deposit for the
execution fee) asks the enclave to clear the batch. This path is wired in the
contract but was not exercised end to end during the hackathon.

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
`scm = H(DOM_SWAP, epoch, pair_id, direction, delta, claim_pk, rseed)`. The
amount is sealed: the circuit also proves `delta_cm = H(DOM_DELTA, delta, r)`
and that `(EPK, ct)` is an encryption of `(delta, r)` to the sealer's Jubjub
key (ECDH and a Poseidon pad, all inside the proof), so the contract stores a
commitment and a ciphertext it cannot open and the amount is unlinkable *and*
hidden. When the epoch (20 blocks) ends, the sealer opens every entry of the
batch, checks each against its commitment, and publishes the per-direction
totals, either signed by the registered executor key (`end_epoch_signed`) or
returned by OutLayer from the same program running inside an Intel TDX
enclave (`end_epoch` + callback). The contract executes the totals against
the pool and publishes `BatchSwapOutputData`. The swapper then proves, in
zero knowledge, that a swap commitment in the tree belongs to them and that
their outputs are the exact pro-rata share of the published result, and the
contract mints those outputs as private notes. Public per swap: the sold side.
Public per batch: the totals. Hidden: who swapped how much, and which claim
belongs to which swap.

## Measured gas (sandbox, nearcore 2.14.0)

| Call | Tgas |
| --- | --- |
| Groth16 verification, BLS12-381 host functions, 8 public inputs | ~17 |
| Poseidon(2) hash in wasm | ~1.5 |
| deposit (`ft_transfer_call` round trip, one tree insert) | 38 total |
| join-split transfer (verify + 2 tree inserts) | 83 |
| join-split withdrawal (same + `ft_transfer`) | 88 total |
| sealed swap (verify 16 public inputs + 2 tree inserts) | 92 |
| end_epoch_signed (signature check + batch execution) | 12 |
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
- **Deposits are public amounts; swap amounts are sealed.** A deposit's amount
  and commitment are visible. A swap reveals the sold side but not the amount:
  only the sealer can open it, and the proof guarantees the ciphertext opens to
  the committed amount. The sealer is trusted for the *amounts* only, not for
  funds: it can publish nothing but totals, and a wrong total is caught by the
  executor signature or the TEE attestation. Run it inside OutLayer for
  hardware isolation, or as the pool operator's signed executor.
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

## Built with

Everything in this repository was written during the hackathon window (first
commit 5 November 2026); there is no earlier codebase behind it. The code was
written with an AI coding assistant (Claude Code), as the commit trailers
show. Libraries it stands on:

- [arkworks](https://arkworks.rs) 0.6: `ark-groth16`, `ark-relations`,
  `ark-r1cs-std`, `ark-crypto-primitives` (Poseidon sponge), `ark-bls12-381`,
  `ark-ed-on-bls12-381` (Jubjub)
- [near-sdk](https://github.com/near/near-sdk-rs) 5.30 and
  near-contract-standards; near-workspaces 0.24 for the sandbox tests and the
  CLI's chain client
- x25519-dalek, chacha20poly1305, ed25519-dalek and sha2 for note encryption
  and executor signatures
- wasm-bindgen and wasm-pack for the browser prover; Vite, TypeScript,
  near-api-js and NEAR Wallet Selector for the web wallet
- The NEAR Intents 1Click API for quotes, and OutLayer's interface for the
  TEE path
- Playwright, ffmpeg, numpy, scipy and edge-tts for the scripted videos

## License

MIT, see `LICENSE`.
