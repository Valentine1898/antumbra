//! Throwaway contract for the gas spike. One method per verification path.
//!
//! Byte layouts (see spike/gen):
//! - BN254 / alt_bn128 host functions: G1 = 64 bytes, G2 = 128 bytes, scalar = 32 bytes LE.
//! - BLS12-381 / NEP-488 host functions: G1 = 96 bytes, G2 = 192 bytes, scalar = 32 bytes LE.
//! - Pure-wasm paths take arkworks canonical uncompressed serialisation.
//!
//! `vk` blob = alpha_g1 || beta_g2 || gamma_g2 || delta_g2 || ic[0..=n]
//! `proof` blob = a || b || c
//! `inputs` blob = n scalars

use ark_ec::pairing::Pairing;
use ark_ff::{BigInteger, PrimeField};
use ark_groth16::{Groth16, PreparedVerifyingKey, Proof};
use ark_serialize::CanonicalDeserialize;
use near_sdk::{env, json_types::Base64VecU8, near};

#[near(contract_state)]
#[derive(Default)]
pub struct Spike {}

struct Layout {
    g1: usize,
    g2: usize,
}

const FR: usize = 32;
const ONE_LE32: [u8; 32] = {
    let mut b = [0u8; 32];
    b[0] = 1;
    b
};

struct Parts<'a> {
    alpha: &'a [u8],
    beta: &'a [u8],
    gamma: &'a [u8],
    delta: &'a [u8],
    ic: &'a [u8],
    a: &'a [u8],
    b: &'a [u8],
    c: &'a [u8],
    inputs: &'a [u8],
}

fn split<'a>(l: &Layout, vk: &'a [u8], proof: &'a [u8], inputs: &'a [u8]) -> Parts<'a> {
    let (g1, g2) = (l.g1, l.g2);
    assert!(vk.len() >= g1 + 3 * g2, "vk too short");
    let ic = &vk[g1 + 3 * g2..];
    assert_eq!(ic.len() % g1, 0, "ic not a multiple of G1");
    assert_eq!(inputs.len() % FR, 0, "inputs not a multiple of 32");
    assert_eq!(
        ic.len() / g1,
        inputs.len() / FR + 1,
        "ic.len() != inputs.len() + 1"
    );
    assert_eq!(proof.len(), 2 * g1 + g2, "proof length");
    Parts {
        alpha: &vk[0..g1],
        beta: &vk[g1..g1 + g2],
        gamma: &vk[g1 + g2..g1 + 2 * g2],
        delta: &vk[g1 + 2 * g2..g1 + 3 * g2],
        ic,
        a: &proof[0..g1],
        b: &proof[g1..g1 + g2],
        c: &proof[g1 + g2..],
        inputs,
    }
}

/// Builds the multiexp input: (ic[0], 1) then (ic[i+1], input_i).
fn multiexp_input(l: &Layout, p: &Parts) -> Vec<u8> {
    let n = p.inputs.len() / FR;
    let mut me = Vec::with_capacity((n + 1) * (l.g1 + FR));
    me.extend_from_slice(&p.ic[0..l.g1]);
    me.extend_from_slice(&ONE_LE32);
    for i in 0..n {
        me.extend_from_slice(&p.ic[(i + 1) * l.g1..(i + 2) * l.g1]);
        me.extend_from_slice(&p.inputs[i * FR..(i + 1) * FR]);
    }
    me
}

/// e(-A, B) * e(alpha, beta) * e(acc, gamma) * e(C, delta) == 1
fn pairing_input(l: &Layout, p: &Parts, neg_a: &[u8], acc: &[u8]) -> Vec<u8> {
    let mut pc = Vec::with_capacity(4 * (l.g1 + l.g2));
    pc.extend_from_slice(neg_a);
    pc.extend_from_slice(p.b);
    pc.extend_from_slice(p.alpha);
    pc.extend_from_slice(p.beta);
    pc.extend_from_slice(acc);
    pc.extend_from_slice(p.gamma);
    pc.extend_from_slice(p.c);
    pc.extend_from_slice(p.delta);
    pc
}

fn verify_wasm<E: Pairing>(pvk: &[u8], proof: &[u8], inputs: &[u8]) -> bool {
    // The prepared key is trusted contract state in a real deployment: skip its validation.
    let pvk = PreparedVerifyingKey::<E>::deserialize_uncompressed_unchecked(pvk).expect("pvk");
    // Proof points come from the user: full on-curve + subgroup checks.
    let proof = Proof::<E>::deserialize_uncompressed(proof).expect("proof");
    let inputs = Vec::<E::ScalarField>::deserialize_uncompressed(inputs).expect("inputs");
    Groth16::<E>::verify_proof(&pvk, &proof, &inputs).expect("verify")
}

#[near]
impl Spike {
    /// Baseline: the cost of a call that does nothing.
    pub fn noop(&self) -> bool {
        true
    }

    /// Groth16 over BN254 through the alt_bn128 host functions.
    pub fn verify_bn254_host(
        &self,
        vk: Base64VecU8,
        proof: Base64VecU8,
        inputs: Base64VecU8,
    ) -> bool {
        let l = Layout { g1: 64, g2: 128 };
        let p = split(&l, &vk.0, &proof.0, &inputs.0);
        let acc = env::alt_bn128_g1_multiexp(multiexp_input(&l, &p));
        let mut neg = Vec::with_capacity(1 + l.g1);
        neg.push(1u8); // sign byte: 1 = negate
        neg.extend_from_slice(p.a);
        let neg_a = env::alt_bn128_g1_sum(neg);
        env::alt_bn128_pairing_check(pairing_input(&l, &p, &neg_a, &acc))
    }

    /// Groth16 over BLS12-381 through the NEP-488 host functions.
    pub fn verify_bls12381_host(
        &self,
        vk: Base64VecU8,
        proof: Base64VecU8,
        inputs: Base64VecU8,
    ) -> bool {
        let l = Layout { g1: 96, g2: 192 };
        let p = split(&l, &vk.0, &proof.0, &inputs.0);
        let Some(acc) = env::bls12381_g1_multiexp(multiexp_input(&l, &p)) else {
            env::log_str("bls12381_g1_multiexp rejected input");
            return false;
        };
        let mut neg = Vec::with_capacity(1 + l.g1);
        neg.push(1u8); // sign byte: 1 = negative
        neg.extend_from_slice(p.a);
        let Some(neg_a) = env::bls12381_p1_sum(neg) else {
            env::log_str("bls12381_p1_sum rejected input");
            return false;
        };
        match env::bls12381_pairing_check(pairing_input(&l, &p, &neg_a, &acc)) {
            Some(ok) => ok,
            None => {
                env::log_str("bls12381_pairing_check rejected input");
                false
            }
        }
    }

    /// Groth16 over BN254 in pure wasm (arkworks), for the host-function vs wasm ratio.
    pub fn verify_bn254_wasm(
        &self,
        pvk: Base64VecU8,
        proof: Base64VecU8,
        inputs: Base64VecU8,
    ) -> bool {
        verify_wasm::<ark_bn254::Bn254>(&pvk.0, &proof.0, &inputs.0)
    }

    /// Groth16 over BLS12-377 in pure wasm (arkworks). BLS12-377 has no host functions on NEAR.
    pub fn verify_bls12377_wasm(
        &self,
        pvk: Base64VecU8,
        proof: Base64VecU8,
        inputs: Base64VecU8,
    ) -> bool {
        verify_wasm::<ark_bls12_377::Bls12_377>(&pvk.0, &proof.0, &inputs.0)
    }

    /// n chained Poseidon(2) hashes over BN254 Fr (light-poseidon, circom parameters).
    pub fn poseidon_bench(&self, n: u32) -> String {
        use light_poseidon::{Poseidon, PoseidonHasher};
        let mut h = Poseidon::<ark_bn254::Fr>::new_circom(2).expect("poseidon params");
        let mut acc = ark_bn254::Fr::from(1u64);
        for i in 0..n {
            acc = h.hash(&[acc, ark_bn254::Fr::from(i as u64)]).expect("hash");
        }
        hex::encode(acc.into_bigint().to_bytes_le())
    }
}
