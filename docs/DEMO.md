# Demo script and submission post

## Video (3–4 minutes, terminal + testnet explorer)

Record the terminal at a readable font size; keep the explorer tab for the
pool account open: `https://testnet.nearblocks.io/address/<pool id>` (ids in
`deployment.json`).

1. **Pitch (20 s).** "Antumbra is a shielded pool with batch swaps on NEAR.
   Groth16 proofs are verified on-chain
   with NEAR's BLS12-381 host functions, 17 Tgas each. Three circuits:
   join-split, swap, swap claim."
2. **Repo (20 s).** Scroll the README status table and `docs/DESIGN.md`
   headings. Mention the gas table.
3. **Pool state (15 s).** `antumbra info`: pool id, root, epoch, the pair with
   its reserves.
4. **Deposit and private transfer (45 s).**
   `antumbra --wallet alice.json deposit --token <t1> --amount 600000`, then
   `sync`. Show the explorer: the deposit is a public `ft_transfer_call` with a
   commitment, no owner. `antumbra --wallet alice.json transfer --asset 1
   --amount 250000 --to <bob address>`; then `antumbra --wallet bob.json sync`
   finds the note by trial decryption. In the explorer the join-split shows
   two nullifiers and two commitments and nothing else.
5. **Swap (45 s).** `antumbra --wallet alice.json swap --pair 1 --sell 1
   --amount 100000`. Explain: the burned amount is public but unlinkable, the
   swap commitment goes into the tree, the epoch batch accumulates. Wait for
   the epoch (20 blocks). `antumbra --wallet alice.json claim` executes the
   batch (one clearing price for everyone) and mints 90 661 of asset 2 as a
   private note. `sync` shows the balance.
6. **Withdraw (20 s).** `antumbra --wallet alice.json withdraw --asset 2
   --amount 90661 --to <account>`; explorer shows the `ft_transfer` to the
   account, the pool reserves moved to 1 100 000 / 909 339.
7. **Verification story (20 s).** `scripts/check.sh` output: unit tests,
   constraint counts, the two sandbox scenarios with their rejected attacks.
8. **Close (15 s).** What is next: sealed amounts through a TEE, routing the
   batch through NEAR Intents, a web wallet. "Built at NEAR HACKATHON."

Use `scripts/demo.sh deployment.json` for a dry run; it prints every command's
gas and the pool state.

## X post (required by the submission form)

> Built at NEAR HACKATHON: Antumbra, a shielded pool with batch swaps on
> @NEARProtocol.
>
> Private transfers, swaps that clear at one price per epoch, and private
> claims, each a single Groth16 proof verified on-chain with NEAR's
> BLS12-381 host functions (17 Tgas). Three circuits, a Rust contract, a CLI
> wallet, live on testnet.
>
> Repo + demo video below. #NEARHackathon

Attach the video. Optional tags: @NearLegion @NearBuilders. Submit the post
link and the repository link in the form at nearhackathon.com; tracks:
privacy, defi, open. The `website` field must stay empty.
