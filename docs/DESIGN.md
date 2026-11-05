# Antumbra design

Shielded batch swaps on NEAR, implemented as one smart contract and
three Groth16 circuits over BLS12-381, verified on-chain through the NEP-488
host functions (17 Tgas per proof). The swap input is burned from a shielded
pool, so it is unlinkable to any account; every swap of an epoch clears at
one price; the swap output is minted privately. Sealed swap amounts are out
of scope for the core and listed as a stretch goal.

Poseidon over BLS12-381 Fr (width 3, 8 full + 57 partial rounds, x^5) is the
only hash; its parameters and the empty-subtree roots are generated once by
`gen-consts` and embedded. The same functions run natively, inside the NEAR
wasm runtime and as R1CS gadgets, with tests pinning them to each other.

## State

- **Asset registry.** NEP-141 token account id -> `asset_id` (u32, from 1).
- **State commitment tree (SCT).** One incremental Poseidon Merkle tree of
  depth 20. Leaves are note commitments and swap commitments, domain
  separated inside the hash. The contract keeps every historical root, so a
  proof built against a slightly stale root still verifies.
- **Nullifier set.** Every spent note and claimed swap.
- **Ciphertext log.** Each leaf is stored with the ciphertext the sender
  attached, indexed by position, so a wallet scans with view calls and
  trial-decrypts.
- **Pairs.** `(asset_1, asset_2, reserve_1, reserve_2)`, a constant-product
  pool with a 0.3% fee. Liquidity is public (`ft_transfer_call` with
  `{"liquidity_pair": id}`); LP privacy is out of scope.
- **Batches.** Per `(epoch, pair)`: accumulated `delta_1`, `delta_2` and,
  once executed, the `BatchSwapOutputData` and its hash.

Amounts are u128 but limited to 112 bits everywhere (circuits range-check
them), so a product of two amounts stays below the field modulus.

## Notes and keys

```
spend key      sk        random scalar (wallet secret)
nullifier key  nk  = H(DOM_NK, sk)
owner key      pk  = H(DOM_PK, sk)            what a note is locked to
note           (asset_id, amount, pk, rseed)
commitment     cm  = H(DOM_NOTE, asset_id, amount, pk, rseed)
nullifier      nf  = H(DOM_NF, nk, cm, position)
```

Delivery uses a separate X25519 key pair per wallet; an address is
`(pk, enc_pk)`. The sender encrypts a tagged plaintext (note: `asset_id,
amount, rseed`; swap: `epoch, pair_id, direction, delta, rseed`) to `enc_pk`
with ChaCha20-Poly1305 under an ephemeral ECDH key, and attaches the blob to
the action. No diversified addresses, no fuzzy message detection.

## Actions

Every action is one NEAR function call carrying one proof. Groth16 public
inputs that no constraint uses are still bound by squaring them.

### Deposit (public)

`ft_transfer_call(pool, amount, msg = { cm, ciphertext })`. The contract
checks the token is registered, inserts `cm`, stores the ciphertext. Amount
and commitment are public; privacy starts inside the pool.

### JoinSplit: private transfer and withdrawal

Public inputs: `root, asset_id, nf_1, nf_2, cm_out_1, cm_out_2, public_amount, ext_hash`.

Private: two input notes with their keys, positions and Merkle paths; two
output notes. Constraints: input commitments recomputed; Merkle membership
under `root`, skipped when the amount is zero (a dummy input, which still
yields a nullifier, so dummies use fresh randomness every time); nullifiers;
output commitments; `amount_in_1 + amount_in_2 + public_amount = amount_out_1
+ amount_out_2`; `ext_hash` bound.

| Variant | public_amount | ext_hash |
| --- | --- | --- |
| transfer | 0 | 0 |
| withdraw | `-amount` (as `p - amount`) | `H(DOM_EXT, sha256(recipient) mod r)`; contract then calls `ft_transfer` |

### Swap

Public inputs: `root, asset_id, nf_1, nf_2, cm_change, scm, epoch, pair_id, direction, delta`.

```
swap record  (epoch, pair_id, direction, delta, claim_pk, rseed)
scm        = H(DOM_SWAP, epoch, pair_id, direction, delta, claim_pk, rseed)
```

Two inputs of `asset_id` are spent exactly as in JoinSplit, one change note
is created, `amount_in_1 + amount_in_2 = change + delta`, `direction ∈ {1,
2}`, `delta > 0`, and `scm` opens to the public `(epoch, pair_id, direction,
delta)` with a witness `claim_pk, rseed`. The contract checks that
`asset_id` is the sold side of the pair, that `epoch` is at most the
current one and its batch has not been executed, records the nullifiers,
inserts `cm_change` then `scm`, and adds `delta` to the batch. The burned
amount is public but unlinkable to the sender.

### Epoch execution (public, permissionless)

Epoch = `block_height / EPOCH_BLOCKS` (20 blocks). Once an epoch has ended,
`end_epoch(epoch, pair)` executes its batch; `swap_claim` does the same on
demand if nobody has. Direction 1 (`delta_1`, asset_1 -> asset_2) executes
first, then direction 2 against the updated reserves, each as one
constant-product trade with the fee. An input that produces no output
(empty pool, or rounding) is returned as `unfilled`.

```
BatchSwapOutputData {
  epoch, pair_id, asset_1, asset_2,
  delta_1, delta_2,        total burned per direction
  lambda_1, lambda_2,      total output per direction (lambda_2 pays delta_1)
  unfilled_1, unfilled_2,  returned unexecuted
}
bsod_hash = H(DOM_BSOD, epoch, pair_id, asset_1, asset_2, delta_1, delta_2, lambda_1, lambda_2, unfilled_1, unfilled_2)
```

Everyone on a side gets the same price. Only batch totals are public; a
swap alone in its batch reveals its own amount.

### SwapClaim

Public inputs: `root, nf_swap, cm_out_1, cm_out_2, bsod_hash`.

Private: the swap record, `sk` (with `claim_pk = H(DOM_PK, sk)`), position
and path, the BSOD fields, output randomness. Constraints: `scm` recomputed
and proven in the SCT; `nf_swap = H(DOM_NF, nk, scm, position)`; `bsod_hash`
recomputed; direction selects `(delta_total, lambda, unfilled, asset_out,
asset_in)`; `out = floor(delta * lambda / delta_total)` and `refund =
floor(delta * unfilled / delta_total)` proven with quotient and remainder
witnesses (`q * total + r = delta * lambda`, `r < total`); `cm_out_1` is the
bought-asset note and `cm_out_2` the refund note, both owned by `claim_pk`.
The contract reveals which batch is claimed (`epoch, pair_id`) but not which
swap, checks the batch is executed, records the nullifier and inserts both
notes.

## Measured gas (sandbox, nearcore 2.14.0)

| Call | Proof | Tgas |
| --- | --- | --- |
| deposit (`ft_transfer_call` round trip) | no | 38 total |
| joinsplit transfer | 1 | 83 |
| joinsplit withdrawal | 1 | 88 total with `ft_transfer` |
| swap | 1 | 86 |
| end_epoch | no | 11 |
| swap_claim | 1 | 81 |

Groth16 verification costs 17 Tgas; a Poseidon permutation in wasm about
1.5 Tgas, and a tree insert is 20 of them, which is where most of the gas
goes. The 300 Tgas limit per call is why there is one proof per action.

## Circuits

| Circuit | Public inputs | Constraints |
| --- | --- | --- |
| joinsplit | 8 | 17 376 |
| swap | 10 | 17 625 |
| swapclaim | 5 | 22 349 |

Setups are seeded (development only, public toxic waste); verifying keys are
embedded in the contract and regenerated byte-for-byte by `antumbra-setup`.

## Client

`antumbra-wallet` holds the wallet state (spend key, delivery key, notes,
swaps, a cache of every leaf commitment for Merkle paths) in one JSON file,
scans the chain by trial decryption, regenerates proving keys from the seeded
setups (cached on disk) and submits actions through near-workspaces. The
`antumbra` CLI exposes it: `keys-new`, `address`, `sync`, `balance`, `info`,
`deposit`, `transfer`, `withdraw`, `swap`, `end-epoch`, `claim`, plus
`deploy`, `mint` and `add-liquidity` for demos. The NEAR account that signs
is visible on-chain as the gas payer; a relayer or a dedicated account breaks
that link.

## Sealed amounts

The chain never learns a swap's amount. The swap circuit proves a
*verifiable encryption* of `(delta, r)` to the sealer's key:

```
sealer key   sk ∈ Jubjub scalars, PK = sk·G          (registered on the contract)
ephemeral    e,  EPK = e·G,  S = e·PK
key          k  = H(DOM_SEALKEY, S.x, S.y)
ciphertext   ct = [delta + H(DOM_SEAL, k, 0), r + H(DOM_SEAL, k, 1)]
commitment   delta_cm = H(DOM_DELTA, delta, r)
```

Jubjub's base field is BLS12-381's scalar field, so the two scalar
multiplications, the Poseidon pad and the commitment are all native in the
circuit (about 6 000 extra constraints). Public inputs of the sealed swap:
`root, asset_id, nf_1, nf_2, cm_change, scm, epoch, pair_id, direction,
delta_cm, EPK.x, EPK.y, ct_0, ct_1, PK.x, PK.y`. The contract checks `PK` is
the registered sealer key, stores `(direction, delta_cm, EPK, ct)` in the
batch and never sees `delta`. Because the proof binds the ciphertext to the
committed amount, the sealer can trust every entry it decrypts.

Clearing an epoch: the sealer receives the batch entries, decrypts them,
recomputes `delta_cm` for each, sums per direction and returns
`(entries_hash, delta_1, delta_2)` where `entries_hash = sha256` over the
entries in order. Two ways to deliver the result:

- **OutLayer (TEE).** `end_epoch` asks `outlayer.testnet` to run the sealer
  program (pinned by its wasm hash) on the batch; OutLayer only accepts
  results from workers attested inside Intel TDX, and the contract's callback
  checks `entries_hash` before executing the batch. The sealer's secret is a
  `PROTECTED_` secret that is generated inside the TEE and injected only into
  that program.
- **Signed executor.** `end_epoch_signed(epoch, pair, delta_1, delta_2,
  signature)` with an ed25519 signature by the registered executor key over
  `sha256("antumbra-sealed-totals-v1" || pool || epoch || pair || entries_hash
  || delta_1 || delta_2)`. Same program, run by the pool operator (or inside
  any other TEE); the trust root is the registered key. Used by the tests and
  the local demo.

Either way the batch then executes exactly as before and the claim circuit is
unchanged: the claimer already knows their own `delta`. What stays public per
swap: the sold side (`asset_id` and `direction`), the nullifiers and the
commitments. What became private: the amount.

## Routing a batch through NEAR Intents

A cleared batch is two totals, `delta_1` and `delta_2`. The two directions
net against each other; only the imbalance needs outside liquidity, and that
is what gets routed. `antumbra-wallet/src/route.rs` implements the pricing:

1. Take the reference price `p0 = price_1 / price_2` from the 1Click token
   list and find the surplus side: `delta_1 - delta_2 / p0` of asset 1, or
   `delta_2 - delta_1 * p0` of asset 2.
2. Ask 1Click for an `EXACT_INPUT` quote of that net leg. The quote shape is
   the one a NEAR-resident pool uses: balances inside the NEAR Intents
   contract on both sides (`depositType`, `recipientType`, `refundType` =
   `INTENTS`), executed as a signed intent. Same-chain quotes with an
   on-chain deposit are not offered for NEAR pairs.
3. The quote sets the clearing price `p` for everyone in the batch:
   `lambda_2 = delta_1 * p`, `lambda_1 = delta_2 / p`. The matched part never
   leaves the pool; only the net leg does.

Mainnet path (designed, not implemented, since 1Click has no testnet): the
pool keeps its reserves inside `intents.near` and registers the executor's
key on that account (`add_public_key`), so the executor can sign intents for
the pool. Clearing becomes `end_epoch_routed(epoch, pair, totals, quote,
signature)`: the contract records the totals and moves the net leg into its
Intents balance; the executor submits the signed intent; when 1Click
settles, the contract withdraws the bought side (`ft_withdraw`) and, in the
callback, publishes `BatchSwapOutputData` at the fill price. The amount the
contract receives is what it publishes, so a dishonest executor cannot
inflate the output. Confidential quotes (`confidentiality: basic`) require a
1Click JWT; the router sends the flag whenever `ONECLICK_JWT` is set.

What exists today: `antumbra route quote` and `antumbra route plan`, the
`--route` flag of `antumbra sealer clear`, and a live quote on the swap page
of the web wallet, all dry runs with real mainnet prices against the testnet
pool.

## Stretch, in order

1. Settle routed batches on mainnet (the contract path above).
2. Hide the sold side as well (a two-asset join-split), so a swap reveals
   nothing but its existence.
