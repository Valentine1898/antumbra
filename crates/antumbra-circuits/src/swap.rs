//! Swap: burn `delta` of one side of a pair from the shielded pool and
//! commit to a swap record that the batch result can later be claimed with.
//!
//! Public inputs, in this order:
//! `root, asset_id, nf_1, nf_2, cm_change, scm, epoch, pair_id, direction, delta`
//!
//! Statement: two input notes of `asset_id` are spent (as in JoinSplit), one
//! change note is created, `amount_in_1 + amount_in_2 = change + delta`, and
//! `scm = H(DOM_SWAP, epoch, pair_id, direction, delta, claim_pk, rseed)` for
//! a witness `claim_pk, rseed`. The contract checks that `asset_id` is the
//! burned side of `pair_id` and that `epoch` is the current one.

use crate::common::{direction_is_1, output_note, range_check_amount, spend_input, InputWitness};
use antumbra_core::{
    gadgets::{domain, hash_var},
    note::{Note, SwapRecord, DOM_SWAP},
    Fr,
};
use ark_ff::Zero;
use ark_r1cs_std::{
    alloc::AllocVar, boolean::Boolean, eq::EqGadget, fields::fp::FpVar, fields::FieldVar,
};
use ark_relations::gr1cs::{ConstraintSynthesizer, ConstraintSystemRef, SynthesisError};

pub const N_PUBLIC: usize = 10;

#[derive(Clone, Debug)]
pub struct SwapPublic {
    pub root: Fr,
    pub asset_id: u32,
    pub nullifiers: [Fr; 2],
    pub change_commitment: Fr,
    pub swap_commitment: Fr,
    pub epoch: u64,
    pub pair_id: u32,
    pub direction: u8,
    pub delta: u128,
}

impl SwapPublic {
    pub fn to_inputs(&self) -> Vec<Fr> {
        vec![
            self.root,
            Fr::from(self.asset_id as u64),
            self.nullifiers[0],
            self.nullifiers[1],
            self.change_commitment,
            self.swap_commitment,
            Fr::from(self.epoch),
            Fr::from(self.pair_id as u64),
            Fr::from(self.direction as u64),
            Fr::from(self.delta),
        ]
    }
}

#[derive(Clone, Debug)]
pub struct SwapCircuit {
    pub public: SwapPublic,
    pub inputs: [InputWitness; 2],
    pub change: Note,
    pub swap: SwapRecord,
}

impl SwapCircuit {
    pub fn blank() -> Self {
        let dummy = InputWitness::dummy(0, Fr::zero());
        let swap = SwapRecord {
            epoch: 0,
            pair_id: 0,
            direction: 1,
            delta: 0,
            claim_pk: Fr::zero(),
            rseed: Fr::zero(),
        };
        SwapCircuit {
            public: SwapPublic {
                root: Fr::zero(),
                asset_id: 0,
                nullifiers: [Fr::zero(); 2],
                change_commitment: Fr::zero(),
                swap_commitment: Fr::zero(),
                epoch: 0,
                pair_id: 0,
                direction: 1,
                delta: 0,
            },
            inputs: [dummy.clone(), dummy],
            change: Note::dummy(0, Fr::zero(), Fr::zero()),
            swap,
        }
    }

    pub fn new(
        root: Fr,
        asset_id: u32,
        inputs: [InputWitness; 2],
        change: Note,
        swap: SwapRecord,
    ) -> Self {
        let public = SwapPublic {
            root,
            asset_id,
            nullifiers: [inputs[0].nullifier(), inputs[1].nullifier()],
            change_commitment: change.commitment(),
            swap_commitment: swap.commitment(),
            epoch: swap.epoch,
            pair_id: swap.pair_id,
            direction: swap.direction,
            delta: swap.delta,
        };
        SwapCircuit {
            public,
            inputs,
            change,
            swap,
        }
    }
}

impl ConstraintSynthesizer<Fr> for SwapCircuit {
    fn generate_constraints(self, cs: ConstraintSystemRef<Fr>) -> Result<(), SynthesisError> {
        let p = &self.public;
        let root = FpVar::new_input(cs.clone(), || Ok(p.root))?;
        let asset_id = FpVar::new_input(cs.clone(), || Ok(Fr::from(p.asset_id as u64)))?;
        let nf_pub = [
            FpVar::new_input(cs.clone(), || Ok(p.nullifiers[0]))?,
            FpVar::new_input(cs.clone(), || Ok(p.nullifiers[1]))?,
        ];
        let cm_change = FpVar::new_input(cs.clone(), || Ok(p.change_commitment))?;
        let scm_pub = FpVar::new_input(cs.clone(), || Ok(p.swap_commitment))?;
        let epoch = FpVar::new_input(cs.clone(), || Ok(Fr::from(p.epoch)))?;
        let pair_id = FpVar::new_input(cs.clone(), || Ok(Fr::from(p.pair_id as u64)))?;
        let direction = FpVar::new_input(cs.clone(), || Ok(Fr::from(p.direction as u64)))?;
        let delta = FpVar::new_input(cs.clone(), || Ok(Fr::from(p.delta)))?;

        let mut sum_in = FpVar::<Fr>::zero();
        for (inp, nf_expected) in self.inputs.iter().zip(nf_pub.iter()) {
            sum_in += spend_input(cs.clone(), inp, &asset_id, &root, nf_expected)?;
        }
        let change = output_note(cs.clone(), &self.change, &asset_id, &cm_change)?;

        let _ = direction_is_1(&direction)?;
        range_check_amount(&delta)?;
        delta.is_zero()?.enforce_equal(&Boolean::FALSE)?;

        let claim_pk = FpVar::new_witness(cs.clone(), || Ok(self.swap.claim_pk))?;
        let rseed = FpVar::new_witness(cs.clone(), || Ok(self.swap.rseed))?;
        let scm = hash_var(
            cs.clone(),
            &[
                domain(DOM_SWAP),
                epoch,
                pair_id,
                direction,
                delta.clone(),
                claim_pk,
                rseed,
            ],
        )?;
        scm.enforce_equal(&scm_pub)?;

        sum_in.enforce_equal(&(change + delta))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use antumbra_core::{note::SpendKey, tree};
    use ark_relations::gr1cs::ConstraintSystem;

    fn sample() -> SwapCircuit {
        let alice = SpendKey(Fr::from(11u64));
        let a = Note {
            asset_id: 1,
            amount: 600,
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
            InputWitness::dummy(1, Fr::from(9u64)),
        ];
        let change = Note {
            asset_id: 1,
            amount: 500,
            pk: alice.pk(),
            rseed: Fr::from(3u64),
        };
        let swap = SwapRecord {
            epoch: 7,
            pair_id: 1,
            direction: 1,
            delta: 100,
            claim_pk: alice.pk(),
            rseed: Fr::from(4u64),
        };
        SwapCircuit::new(tree::root_of(&leaves), 1, inputs, change, swap)
    }

    #[test]
    fn satisfied_and_constraint_count() {
        let cs = ConstraintSystem::<Fr>::new_ref();
        sample().generate_constraints(cs.clone()).unwrap();
        assert!(cs.is_satisfied().unwrap());
        assert_eq!(cs.num_instance_variables(), N_PUBLIC + 1);
        println!("swap constraints: {}", cs.num_constraints());
    }

    #[test]
    fn mismatched_public_delta_is_rejected() {
        let mut c = sample();
        c.public.delta = 90; // burn less publicly than the swap record says
        let cs = ConstraintSystem::<Fr>::new_ref();
        c.generate_constraints(cs.clone()).unwrap();
        assert!(!cs.is_satisfied().unwrap());
    }
}
