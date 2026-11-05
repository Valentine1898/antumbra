# Keys

`joinsplit.vk.near`: Groth16 verifying key for the join-split circuit, in the
NEP-488 byte layout, embedded into the pool contract at build time.

Regenerate with `cargo run -p antumbra-circuits --bin antumbra-setup --release`.
The setup is seeded, so the output is byte-for-byte reproducible. This is a
development setup with public toxic waste; a production deployment needs a
multi-party ceremony.

Proving keys are not stored: the wallet regenerates them from the same seed.
