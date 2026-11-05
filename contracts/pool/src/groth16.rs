//! Groth16 verification over BLS12-381 through the NEP-488 host functions.
//!
//! vk    = alpha_g1 || beta_g2 || gamma_g2 || delta_g2 || ic[0..=n]
//! proof = a || b || c
//! Checks e(-A, B) * e(alpha, beta) * e(acc, gamma) * e(C, delta) == 1 with
//! acc = ic[0] + sum(input_i * ic[i+1]).

use near_sdk::env;

pub const G1: usize = 96;
pub const G2: usize = 192;
pub const SCALAR: usize = 32;
pub const PROOF_LEN: usize = 2 * G1 + G2;

const ONE_LE32: [u8; 32] = {
    let mut b = [0u8; 32];
    b[0] = 1;
    b
};

/// Returns `Some(false)` on a wrong statement and `None` when the host rejects the input encoding.
pub fn verify(vk: &[u8], proof: &[u8], inputs: &[[u8; SCALAR]]) -> Option<bool> {
    if proof.len() != PROOF_LEN || vk.len() < G1 + 3 * G2 {
        return None;
    }
    let ic = &vk[G1 + 3 * G2..];
    if ic.len() % G1 != 0 || ic.len() / G1 != inputs.len() + 1 {
        return None;
    }
    let (alpha, beta) = (&vk[0..G1], &vk[G1..G1 + G2]);
    let (gamma, delta) = (&vk[G1 + G2..G1 + 2 * G2], &vk[G1 + 2 * G2..G1 + 3 * G2]);
    let (a, b, c) = (&proof[0..G1], &proof[G1..G1 + G2], &proof[G1 + G2..]);

    let mut me = Vec::with_capacity((inputs.len() + 1) * (G1 + SCALAR));
    me.extend_from_slice(&ic[0..G1]);
    me.extend_from_slice(&ONE_LE32);
    for (i, x) in inputs.iter().enumerate() {
        me.extend_from_slice(&ic[(i + 1) * G1..(i + 2) * G1]);
        me.extend_from_slice(x);
    }
    let acc = env::bls12381_g1_multiexp(me)?;

    let mut neg = Vec::with_capacity(1 + G1);
    neg.push(1u8);
    neg.extend_from_slice(a);
    let neg_a = env::bls12381_p1_sum(neg)?;

    let mut pairs = Vec::with_capacity(4 * (G1 + G2));
    pairs.extend_from_slice(&neg_a);
    pairs.extend_from_slice(b);
    pairs.extend_from_slice(alpha);
    pairs.extend_from_slice(beta);
    pairs.extend_from_slice(&acc);
    pairs.extend_from_slice(gamma);
    pairs.extend_from_slice(c);
    pairs.extend_from_slice(delta);
    env::bls12381_pairing_check(pairs)
}
