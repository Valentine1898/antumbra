//! Key generation, proving and native verification.
//!
//! The setups are deterministic (fixed seeds) so anyone can regenerate the
//! exact keys the contract embeds. That makes them *development* setups: the
//! toxic waste is public. A real deployment needs an MPC ceremony.

use ark_groth16::{prepare_verifying_key, Groth16, Proof, ProvingKey, VerifyingKey};
use ark_relations::gr1cs::ConstraintSynthesizer;
use ark_snark::SNARK;
use ark_std::rand::{rngs::StdRng, SeedableRng};
use sha2::{Digest, Sha256};

use crate::{
    joinsplit::JoinSplitCircuit, near_encoding, swap::SwapCircuit, swapclaim::SwapClaimCircuit,
    Bls12_381, Fr,
};

pub type Keys = (ProvingKey<Bls12_381>, VerifyingKey<Bls12_381>);

pub const JOINSPLIT_SETUP_SEED: [u8; 32] = *b"antumbra-dev-setup-joinsplit-v01";
pub const SWAP_SETUP_SEED: [u8; 32] = *b"antumbra-dev-setup-swap-v0000001";
pub const SWAPCLAIM_SETUP_SEED: [u8; 32] = *b"antumbra-dev-setup-swapclaim-v01";

fn setup<C: ConstraintSynthesizer<Fr>>(blank: C, seed: [u8; 32]) -> Keys {
    let mut rng = StdRng::from_seed(seed);
    Groth16::<Bls12_381>::circuit_specific_setup(blank, &mut rng).expect("setup")
}

pub fn joinsplit_setup() -> Keys {
    setup(JoinSplitCircuit::blank(), JOINSPLIT_SETUP_SEED)
}

pub fn swap_setup() -> Keys {
    setup(SwapCircuit::blank(), SWAP_SETUP_SEED)
}

pub fn swapclaim_setup() -> Keys {
    setup(SwapClaimCircuit::blank(), SWAPCLAIM_SETUP_SEED)
}

pub fn prove<C: ConstraintSynthesizer<Fr>>(
    pk: &ProvingKey<Bls12_381>,
    circuit: C,
    seed: [u8; 32],
) -> Proof<Bls12_381> {
    let mut rng = StdRng::from_seed(seed);
    Groth16::<Bls12_381>::prove(pk, circuit, &mut rng).expect("prove")
}

pub fn verify(vk: &VerifyingKey<Bls12_381>, inputs: &[Fr], proof: &Proof<Bls12_381>) -> bool {
    let pvk = prepare_verifying_key(vk);
    Groth16::<Bls12_381>::verify_proof(&pvk, proof, inputs).expect("verify")
}

/// A verifying key in the NEP-488 layout the contract embeds, plus its sha256.
pub fn vk_near(vk: &VerifyingKey<Bls12_381>) -> (Vec<u8>, [u8; 32]) {
    let bytes = near_encoding::vk_bytes(vk);
    let hash: [u8; 32] = Sha256::digest(&bytes).into();
    (bytes, hash)
}

/// `(file stem, bytes, sha256)` for every circuit.
pub fn all_vks_near() -> Vec<(&'static str, Vec<u8>, [u8; 32])> {
    let (_, js) = joinsplit_setup();
    let (_, sw) = swap_setup();
    let (_, sc) = swapclaim_setup();
    [("joinsplit", js), ("swap", sw), ("swapclaim", sc)]
        .into_iter()
        .map(|(name, vk)| {
            let (bytes, hash) = vk_near(&vk);
            (name, bytes, hash)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::joinsplit::InputWitness;
    use antumbra_core::{
        note::{Note, SpendKey},
        tree, Fr,
    };
    use ark_ff::Zero;

    #[test]
    fn prove_and_verify_transfer() {
        let (pk, vk) = joinsplit_setup();
        let alice = SpendKey(Fr::from(11u64));
        let a = Note {
            asset_id: 1,
            amount: 100,
            pk: alice.pk(),
            rseed: Fr::from(1u64),
        };
        let leaves = vec![a.commitment()];
        let inputs = [
            InputWitness {
                note: a,
                sk: alice,
                position: 0,
                siblings: tree::merkle_path(&leaves, 0),
            },
            InputWitness::dummy(1, Fr::from(5u64)),
        ];
        let outputs = [
            Note {
                asset_id: 1,
                amount: 60,
                pk: alice.pk(),
                rseed: Fr::from(2u64),
            },
            Note {
                asset_id: 1,
                amount: 40,
                pk: alice.pk(),
                rseed: Fr::from(3u64),
            },
        ];
        let circuit = JoinSplitCircuit::new(
            tree::root_of(&leaves),
            1,
            inputs,
            outputs,
            Fr::zero(),
            Fr::zero(),
        );
        let public = circuit.public.to_inputs();
        let proof = prove(&pk, circuit, [7u8; 32]);
        assert!(verify(&vk, &public, &proof));
        let mut bad = public.clone();
        bad[7] = Fr::from(1u64);
        assert!(!verify(&vk, &bad, &proof));
    }
}
