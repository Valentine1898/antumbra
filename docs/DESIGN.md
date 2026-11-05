# Antumbra design (draft, day 1)

Target: Penumbra V1 swap semantics on NEAR, implemented as one smart contract
plus two Groth16 circuits. The swap input is burned from a shielded pool, so it
is unlinkable to any account; every swap of an epoch clears at one price; the
swap output is minted privately. Sealed amounts (Penumbra's flow encryption)
are out of scope for the core and listed as a stretch goal.

Curve and hash follow the gas spike (`spike/`, results in the README): the
proposal is BLS12-381 via the NEP-488 host functions (17 Tgas per Groth16
verification, half the cost of `alt_bn128`) and Poseidon over BLS12-381 Fr,
with the same Poseidon parameters used natively in the contract and inside the
circuits. Pure-wasm verification of Penumbra's BLS12-377 proofs costs 130 Tgas,
which rules out multi-proof transactions under NEAR's 300 Tgas limit, so the
protocol has one proof per action and its own circuits.

## State

- **Asset registry.** NEP-141 token account id -> `asset_id` (u32). The pool
  only holds registered tokens.
- **State commitment tree (SCT).** One incremental Merkle tree, Poseidon(2),
  depth 20 (~1M leaves). Leaves are note commitments *and* swap commitments,
  domain-separated inside the hash. The contract keeps the last 64 roots so a
  proof built against a slightly stale root still verifies.
- **Nullifier set.** `LookupSet<[u8; 32]>`.
- **Ciphertext log.** Every inserted commitment is stored with the encrypted
  note (or swap plaintext) the sender attached, indexed by leaf position, so a
  wallet can scan with view calls and trial-decrypt.
- **Pools.** Per trading pair a constant-product pool `(reserve_1, reserve_2)`
  owned by the contract. Liquidity is seeded publicly (a public `add_liquidity`
  call); LP privacy is out of scope.
- **Epoch batches.** Per `(epoch, pair)`: `delta_1`, `delta_2` accumulated from
  swap actions, and after execution the `BatchSwapOutputData`.

## Notes and keys

```
spend key      sk        random scalar (wallet secret)
nullifier key  nk  = Poseidon(DOM_NK, sk)
owner key      pk  = Poseidon(DOM_PK, sk)            (what a note is locked to)
note           (asset_id, amount, pk, rseed)
commitment     cm  = Poseidon(DOM_NOTE, asset_id, amount, pk, rseed)
nullifier      nf  = Poseidon(DOM_NF, nk, cm, position)
```

Delivery uses a separate x25519 key pair per wallet; an "address" is
`(pk, enc_pk)`. The sender encrypts `(asset_id, amount, rseed)` to `enc_pk`
(ECDH + ChaCha20-Poly1305) and attaches the ciphertext to the action.

No diversified addresses, no fuzzy message detection: cut for the hackathon.

## Actions

Every action is one NEAR function call carrying a proof. Groth16 public inputs
that are not used by any constraint must still be constrained (squared) so the
proof binds to them.

### Deposit (public)

`ft_transfer_call(pool, amount, msg = { cm, ciphertext })`. The contract
checks the token is registered, inserts `cm`, stores the ciphertext. Amount and
recipient commitment are public, like a transfer into Penumbra.

### JoinSplit (private transfer, withdrawal, swap input)

Circuit `joinsplit`, 2 inputs, 2 outputs, single asset.

Public inputs:
`root, asset_id, nf_1, nf_2, cm_out_1, cm_out_2, public_amount, ext_hash`

Private: for each input, `(amount, sk, rseed, position, merkle path)`; for each
output, `(amount, pk, rseed)`.

Constraints:
- `cm_in_i = Poseidon(DOM_NOTE, asset_id, amount_i, Poseidon(DOM_PK, sk_i), rseed_i)`
- Merkle membership of `cm_in_i` at `position_i` under `root`, skipped when
  `amount_i == 0` (dummy input). A dummy still has a nullifier, which the
  contract records, so dummies are built from fresh randomness every time.
- `nf_i = Poseidon(DOM_NF, Poseidon(DOM_NK, sk_i), cm_in_i, position_i)`
- `cm_out_j = Poseidon(DOM_NOTE, asset_id, amount_out_j, pk_j, rseed_j)`
- `amount_in_1 + amount_in_2 + public_amount = amount_out_1 + amount_out_2`
  with every amount range-checked to 64 bits; `public_amount` is a field
  element, negative values are `p - x`.
- `ext_hash * ext_hash = ext_hash_sq` (binding only).

Contract: checks `root` is recent, nullifiers unseen, then inserts both output
commitments and records nullifiers. `public_amount` and `ext_hash` decide the
variant:

| Variant | public_amount | ext_hash |
| --- | --- | --- |
| transfer | 0 | 0 |
| withdraw | `-amount` | `Poseidon(DOM_EXT, recipient_account_hash)`; contract does `ft_transfer` |
| swap | `-delta` | swap commitment `scm` (below); contract inserts `scm` into the SCT and adds `delta` to the epoch batch |

### Swap record

```
swap   = (pair_id, direction, delta, claim_pk, rseed)
scm    = Poseidon(DOM_SWAP, pair_id, direction, delta, claim_pk, rseed)
```

`direction` is 1 or 2 (which side of the pair is burned); a swap burns one
asset only, so a JoinSplit on that asset funds it. The swap plaintext is
encrypted to the claimer's `enc_pk` and stored with `scm`.

### Epoch execution (public, permissionless)

Epoch = `block_height / EPOCH_BLOCKS`. The first call that observes a finished
epoch (or an explicit `end_epoch(pair)`) executes the batch for each pair:
`delta_1` against the pool in direction 1→2, then `delta_2` in direction 2→1,
constant product with a flat fee. Result, exactly Penumbra's shape:

```
BatchSwapOutputData {
  epoch, pair_id,
  delta_1, delta_2,       // total burned per direction
  lambda_1, lambda_2,     // total output per direction (lambda_2 pays delta_1 side)
  unfilled_1, unfilled_2, // returned unexecuted (only when a pool is empty)
}
bsod_hash = Poseidon(DOM_BSOD, epoch, pair_id, delta_1, delta_2, lambda_1, lambda_2, unfilled_1, unfilled_2)
```

Both directions are executed, not netted, so the same uniform price applies
to everyone on a side. Only batch totals are public; a swap in a batch of one
reveals its own amount, the same caveat Penumbra documents.

### SwapClaim (private)

Circuit `swapclaim`. Public inputs:
`root, nf_swap, cm_out_1, cm_out_2, bsod_hash`

Private: swap record, `sk` (claim key), position, merkle path, BSOD fields,
output rseeds.

Constraints:
- `scm` recomputed and proven in the SCT; `claim_pk = Poseidon(DOM_PK, sk)`.
- `nf_swap = Poseidon(DOM_NF, Poseidon(DOM_NK, sk), scm, position)`.
- `bsod_hash` recomputed from the private BSOD fields.
- Pro-rata share, floor division checked with multiplication:
  for direction 1, `out = floor(delta * lambda_2 / delta_1)` and
  `refund = floor(delta * unfilled_1 / delta_1)`; symmetric for direction 2.
- `cm_out_1` is the output asset note `(asset_out, out, claim_pk, rseed_1)`,
  `cm_out_2` the refund note `(asset_in, refund, claim_pk, rseed_2)`.

Contract: checks `bsod_hash` matches the stored batch for that pair and epoch,
nullifier unseen, inserts both output commitments.

## Gas budget (measured on the sandbox, day 2)

| Call | Proofs | Tgas |
| --- | --- | --- |
| deposit | 0 | 38 total for the `ft_transfer_call` round trip (one insert ≈ 20 Poseidon ≈ 30) |
| joinsplit | 1 | 83 (verify 17 + 2 inserts ≈ 60 + base) |
| swapclaim | 1 | expected ≈ 85, same shape as joinsplit |
| end_epoch | 0 | arithmetic only, plus one BSOD hash |

Poseidon costs ≈ 1.5 Tgas per permutation in wasm (ark-crypto-primitives
sponge). The 300 Tgas per-call limit leaves room for one proof plus tree
updates per call, so a user transaction is one action; multi-action
transactions are a sequence of calls. Embedded Poseidon constants and
empty-subtree roots (`gen-consts`) avoid ≈ 50 Tgas of setup per call.

## Client

Rust CLI (`antumbra-cli`): key generation, `scan` (fetch ciphertexts by
position range, trial-decrypt, maintain local note set), `deposit`,
`transfer`, `withdraw`, `swap`, `claim`, `end-epoch`. Proving with arkworks
natively; proving keys generated by a documented dev setup and checked in (or
downloaded) with their hashes.

## Stretch, in order

1. Route the net batch through NEAR Intents (1Click, confidential) instead of
   the internal pool, so liquidity comes from the solver network.
2. Sealed deltas: encrypt `delta` to an enclave key (OutLayer); the enclave
   publishes only batch totals plus an attestation; the contract verifies the
   attestation. This is the piece Penumbra planned as flow encryption.
3. Web UI with a wasm prover.
