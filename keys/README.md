# Keys

Groth16 verifying keys in the NEP-488 byte layout, embedded into the pool
contract at build time:

- `joinsplit.vk.near`: private transfers and withdrawals (8 public inputs)
- `swap.vk.near`: swap action (10 public inputs)
- `swapclaim.vk.near`: swap claim (5 public inputs)

Regenerate with `cargo run -p antumbra-circuits --bin antumbra-setup --release`.
The setups are seeded, so the output is byte-for-byte reproducible. These are
development setups with public toxic waste; a production deployment needs a
multi-party ceremony.

Proving keys are not stored: the wallet regenerates them from the same seeds.
