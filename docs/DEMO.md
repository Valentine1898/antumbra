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
3. **The two realms and the gas payer (20 s).** The page is split: *in the
   light* (your NEAR account, public) and *in the shade* (your shielded
   balance, only you can see it). Create a funded testnet dev account in one
   click (key generated in the browser); the light side fills in. Press *wrap
   1 NEAR* and *get 100 USDC.e*: real testnet tokens, wNEAR and USDC.e.
4. **Into the shade, and a private transfer (45 s).** Shield 0.5 wNEAR: a
   mote of light crosses the seam, the shielded balance unveils; the log shows
   the public amount and commitment, the explorer shows an `ft_transfer_call`
   with no owner. Switch the threshold to *To another shade* and send 0.2
   wNEAR to a second wallet's address (open the app in a private window for
   Bob, copy its address). In Bob's window press Sync: the note is found by
   trial decryption. The log and the explorer show two nullifiers and two
   commitments, nothing else.
5. **Swap in the shade (45 s).** Sell 0.1 wNEAR: the proof is built in the
   browser in about three seconds. Explain: the chain stores a commitment and
   a ciphertext only the sealer can open; Geek mode shows the batch with one
   sealed entry and no amount. The swap card says *sealed*; when the epoch
   ends the sealer clears it (`antumbra sealer watch` in a terminal, or the
   TEE), the card turns *cleared* and the page claims the USDC.e by itself (a
   wallet user presses *Claim*). The bought USDC.e appears in the shade.
6. **Out of the shade (20 s).** Unshield the USDC.e to the NEAR account; the
   explorer shows the `ft_transfer`, the pool reserves moved, the light side
   now holds USDC.e.
7. **Verification story (20 s).** `scripts/check.sh` output: unit tests,
   constraint counts, the two sandbox scenarios with their rejected attacks.
8. **Close (15 s).** What is next: routing each batch through NEAR Intents
   (Confidential Intents has a `confidential` flag in the same 1Click API),
   hiding the sold side too. "Built at NEAR HACKATHON."

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
