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
3. **The two realms and the gas payer (20 s).** The side card: *in the
   light* is the NEAR account and what it holds in public, *in the shade* is
   the shielded address and balances only you can see. Create a funded
   testnet dev account in one click (key generated in the browser); the lit
   block fills in. Press *wrap 1 NEAR* and *get 100 USDC.e*: real testnet
   tokens.
4. **Into the shade, and a private transfer (45 s).** Shield 0.5 wNEAR: a
   mote of light crosses the seam, the shielded balance unveils; the log shows
   the public amount and commitment, the explorer shows an `ft_transfer_call`
   with no owner. Switch the threshold to *To another shade* and send 0.2
   wNEAR to a second wallet's address (open the app in a private window for
   Bob, copy its address). In Bob's window press Sync: the note is found by
   trial decryption. The log and the explorer show two nullifiers and two
   commitments, nothing else.
5. **Swap page (45 s).** Type 0.1 wNEAR: the page shows the live NEAR Intents
   quote for the batch's net leg (mainnet prices, dry run). Seal and swap: the
   proof is built in the browser in about three seconds. Explain: the chain stores a commitment and
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

## Recording it

`scripts/video/` records this flow by itself: a Playwright Chromium drives the
wallet against the testnet pool with a drawn cursor, then `compose.py` adds
the intro and outro cards and the step captions and speeds up the wait for
the sealer. See `scripts/video/README.md`; the result is a 1080p MP4 just
over two minutes long.

## X post (required by the submission form)

The post says the project was built at NEAR HACKATHON, tags @NEARProtocol and
#NEARHackathon, and carries the video. Attach both videos to the first post
(X takes up to four media items): the animated presentation first, then the
recorded wallet flow, so the post shows the idea and one complete user action
from click to result. Each text below fits the 280-character limit.

Post:

> On a public chain, anyone can read your balance, your trades and who you trade with.
> 
> Antumbra is a shielded pool with batch swaps on @NEARProtocol: private transfers, sealed swap amounts, one price per epoch.
> 
> Built at NEAR HACKATHON. Demo below. #NEARHackathon

Reply 1, what NEAR makes possible:

> What NEAR makes possible: its native BLS12-381 host functions verify a Groth16 proof on chain for 17 Tgas, so every private action is one proof in one transaction.
> 
> A sealer built for a TEE opens each epoch's batch, and the net leg is priced on NEAR Intents (a dry run for now).

Reply 2, where to try it and a question:

> Try it on testnet. Proofs are built in your browser, about 3 s each:
> https://valentine1898.github.io/antumbra/
> 
> Circuits, contract, wallet and tests:
> https://github.com/Valentine1898/antumbra
> 
> Next: hiding which side you sell. What else should a private DEX hide?
> @NearLegion @NearBuilders

## Submission form (nearhackathon.com/#apply)

One submission per project, before 14 November 2026 23:59 UTC, and it cannot
be edited afterwards (mistakes go through nearhackathon.com/request-changes).
Send it when the repository is public and the post is up.

| Field | Value |
|---|---|
| Project name | Antumbra |
| Team lead, email, team size | yours |
| Tracks | Privacy & Private AI, Unstoppable DeFi, Open Innovation |
| NEAR wallet address | your mainnet account for the prize (`name.near`), not a testnet one |
| Repository link | https://github.com/Valentine1898/antumbra |
| Demo post link | the link to the X post above |
| Website | leave empty: it is a spam trap, any value rejects the submission |

What did you build (20 to 2,000 characters):

> Problem: on a public chain every balance, trade and counterparty is visible forever, and visible orders get front-run.
> 
> Antumbra is a shielded pool with batch swaps on NEAR. You deposit NEP-141 tokens into private notes, transfer them privately, and swap inside the pool. Swaps are collected per 20-block epoch and every amount is sealed: verifiably encrypted to a sealer inside the proof, so the chain stores only a commitment and a ciphertext. When the epoch ends the sealer publishes only the batch totals, the whole batch clears at one price, and each trader claims a share with a proof that does not reveal which swap was theirs.
> 
> What is built: three Groth16 circuits over BLS12-381 (join-split 17,376 constraints, sealed swap 24,330, swap claim 22,349); a Rust contract where every action is one proof in one call (81 to 92 Tgas); a browser wallet that proves in a Web Worker in about 3 seconds; a CLI wallet; a sealer that runs as a signed executor today and is wired to run in a TEE through OutLayer; sandbox end-to-end tests in which replayed claims, an impostor sealer, wrong totals and late swaps are rejected. Live on testnet with wNEAR and USDC.e.
> 
> How NEAR fits: the design rests on NEAR's native BLS12-381 host functions (NEP-488). A Groth16 verification costs 17 Tgas, so a full private action fits in one transaction. Each batch's net leg is priced on NEAR Intents through the 1Click API; today that is a dry run with live mainnet quotes, and settlement through the Intents contract is designed, not shipped. The trusted setup is a seeded development setup, documented as not production-safe.

The hosted wallet should stay up through judging (16 to 18 November, ideally
to the 25th), which means the pool's sealer has to keep running. A scheduled
workflow (`.github/workflows/sealer.yml`) clears finished epochs every few
minutes; `scripts/sealer.sh` on your own machine clears them within seconds,
which is what you want while recording or presenting.
