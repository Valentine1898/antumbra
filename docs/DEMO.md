# Demo script and submission post

## Video (3–4 minutes, browser wallet + testnet explorer)

Record the browser wallet (`cd web && npm run dev`, http://localhost:5173) at a
readable zoom; keep the explorer tab for the pool account open:
`https://testnet.nearblocks.io/address/<pool id>` (ids in `deployment.json`).
Every step below has a button in the wallet and prints what the chain saw in
the log panel; the CLI (`scripts/demo.sh`) is the fallback.

1. **Pitch (20 s).** "Antumbra is a shielded pool with batch swaps on NEAR.
   Groth16 proofs are verified on-chain
   with NEAR's BLS12-381 host functions, 17 Tgas each. Three circuits:
   join-split, swap, swap claim."
2. **Repo (20 s).** Scroll the README status table and `docs/DESIGN.md`
   headings. Mention the gas table.
3. **Pool state and gas payer (20 s).** The pool card: epoch countdown, the
   pair's reserves. Create a funded testnet dev account in one click (key
   generated in the browser) and mint test tokens.
4. **Deposit and private transfer (45 s).** Deposit 600 000 of token 1; the
   log shows the public amount and commitment, the explorer shows an
   `ft_transfer_call` with no owner. Transfer 250 000 to a second wallet's
   address (open the app in a private window for Bob, copy its address). In
   Bob's window press Sync: the note is found by trial decryption. The log
   and the explorer show two nullifiers and two commitments, nothing else.
5. **Swap (45 s).** Swap 100 000 of asset 1: the proof is built in the browser
   in about three seconds. Explain: the chain stores a commitment and a
   ciphertext only the sealer can open; Geek mode shows the batch with one
   sealed entry and no amount. When the epoch ends the sealer clears it
   (`antumbra sealer watch` running in a terminal, or the TEE) and the batch
   totals appear. Press Claim: the bought asset appears as a private note.
6. **Withdraw (20 s).** Withdraw the bought asset to the NEAR account; the
   explorer shows the `ft_transfer`, the pool reserves moved.
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
> Private transfers, swaps with sealed amounts that clear at one price per
> epoch, and private claims, each a single Groth16 proof verified on-chain
> with NEAR's BLS12-381 host functions (17 Tgas). Three circuits, a Rust
> contract, a browser wallet that proves in a Web Worker, a sealer that runs
> in a TEE. Live on testnet.
>
> Repo + demo video below. #NEARHackathon

Attach the video. Optional tags: @NearLegion @NearBuilders. Submit the post
link and the repository link in the form at nearhackathon.com; tracks:
privacy, defi, open. The `website` field must stay empty.
