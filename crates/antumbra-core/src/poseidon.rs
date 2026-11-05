//! Poseidon over BLS12-381 Fr: width 3 (rate 2, capacity 1), x^5 S-box,
//! 8 full and 57 partial rounds. Parameters come from the Grain LFSR procedure
//! of the Poseidon paper, as implemented in ark-crypto-primitives. They are
//! generated once by `gen-consts` and embedded (`consts.rs`), so a contract
//! call only deserialises 204 field elements instead of running the LFSR.
//!
//! `hash` is a plain sponge: absorb all inputs, squeeze one element. There is
//! no length padding, so `hash(&[x])` equals `hash(&[x, 0])`. The protocol
//! therefore only ever hashes fixed-arity tuples, each starting with a domain
//! tag (see `note`), and the circuits enforce the same shapes.

use crate::{consts, encoding::fr_from_bytes};
use ark_bls12_381::Fr;
use ark_crypto_primitives::sponge::{
    poseidon::{find_poseidon_ark_and_mds, PoseidonConfig, PoseidonSponge},
    CryptographicSponge, FieldBasedCryptographicSponge,
};
use ark_ff::PrimeField;
use std::sync::OnceLock;

pub const FULL_ROUNDS: usize = 8;
pub const PARTIAL_ROUNDS: usize = 57;
pub const ALPHA: u64 = 5;
pub const RATE: usize = 2;
pub const CAPACITY: usize = 1;

/// Derives the parameters from scratch (Grain LFSR). Used by `gen-consts` and tests.
pub fn generate_config() -> PoseidonConfig<Fr> {
    let (ark, mds) = find_poseidon_ark_and_mds::<Fr>(
        Fr::MODULUS_BIT_SIZE as u64,
        RATE,
        FULL_ROUNDS as u64,
        PARTIAL_ROUNDS as u64,
        0,
    );
    PoseidonConfig::new(FULL_ROUNDS, PARTIAL_ROUNDS, ALPHA, mds, ark, RATE, CAPACITY)
}

/// The shared parameter set, decoded from the embedded constants once per process.
pub fn config() -> &'static PoseidonConfig<Fr> {
    static CFG: OnceLock<PoseidonConfig<Fr>> = OnceLock::new();
    CFG.get_or_init(|| {
        let dec = |b: &[u8; 32]| fr_from_bytes(b).expect("embedded constant is canonical");
        let ark: Vec<Vec<Fr>> = consts::POSEIDON_ARK
            .iter()
            .map(|row| row.iter().map(dec).collect())
            .collect();
        let mds: Vec<Vec<Fr>> = consts::POSEIDON_MDS
            .iter()
            .map(|row| row.iter().map(dec).collect())
            .collect();
        PoseidonConfig::new(FULL_ROUNDS, PARTIAL_ROUNDS, ALPHA, mds, ark, RATE, CAPACITY)
    })
}

/// Poseidon sponge hash of a list of field elements.
pub fn hash(inputs: &[Fr]) -> Fr {
    hash_with(config(), inputs)
}

/// Same as `hash`, with an explicit parameter set (used when generating the constants).
pub fn hash_with(cfg: &PoseidonConfig<Fr>, inputs: &[Fr]) -> Fr {
    let mut sponge = PoseidonSponge::<Fr>::new(cfg);
    sponge.absorb(&inputs);
    sponge.squeeze_native_field_elements(1)[0]
}

/// Two-to-one hash used by the commitment tree.
pub fn hash2(left: Fr, right: Fr) -> Fr {
    hash(&[left, right])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_constants_match_generation() {
        let gen = generate_config();
        let cfg = config();
        assert_eq!(
            gen.ark, cfg.ark,
            "run `cargo run -p antumbra-core --bin gen-consts --release`"
        );
        assert_eq!(
            gen.mds, cfg.mds,
            "run `cargo run -p antumbra-core --bin gen-consts --release`"
        );
    }

    #[test]
    fn deterministic_and_input_sensitive() {
        let a = hash(&[Fr::from(1u64), Fr::from(2u64)]);
        let b = hash(&[Fr::from(1u64), Fr::from(2u64)]);
        let c = hash(&[Fr::from(2u64), Fr::from(1u64)]);
        assert_eq!(a, b);
        assert_ne!(a, c);
        // Fixed-arity primitive: a trailing zero is not distinguished (no padding).
        assert_eq!(
            hash(&[Fr::from(1u64)]),
            hash(&[Fr::from(1u64), Fr::from(0u64)])
        );
        assert_ne!(
            hash(&[Fr::from(1u64)]),
            hash(&[Fr::from(0u64), Fr::from(1u64)])
        );
    }
}
