//! Swap with a sealed amount: burn `delta` of one side of a pair from the
//! shielded pool, commit to a swap record, and prove that the ciphertext
//! attached for the sealer opens to exactly that `delta`.
//!
//! Public inputs, in this order:
//! `root, asset_id, nf_1, nf_2, cm_change, scm, epoch, pair_id, direction,
//!  delta_cm, epk_x, epk_y, ct_0, ct_1, pk_x, pk_y`
//!
//! Statement: two input notes of `asset_id` are spent (as in JoinSplit), one
//! change note is created, `amount_in_1 + amount_in_2 = change + delta` for a
//! private `delta > 0`, `scm = H(DOM_SWAP, epoch, pair_id, direction, delta,
//! claim_pk, rseed)`, `delta_cm = H(DOM_DELTA, delta, r)`, and
//! `(EPK, ct) = seal(PK, delta, r, e)` for a private ephemeral `e`, with
//! `PK` the sealer key the contract registered.

use crate::common::{direction_is_1, output_note, range_check_amount, spend_input, InputWitness};
use antumbra_core::{
    gadgets::{domain, hash_var, seal_var},
    note::{Note, SwapRecord, DOM_SWAP},
    sealing::{self, Point, SealerKey},
    Fr,
};
use ark_ed_on_bls12_381::{constraints::EdwardsVar, Fr as JubjubScalar};
use ark_ff::Zero;
use ark_r1cs_std::{
    alloc::AllocVar, boolean::Boolean, eq::EqGadget, fields::fp::FpVar, fields::FieldVar,
};
use ark_relations::gr1cs::{ConstraintSynthesizer, ConstraintSystemRef, SynthesisError};

pub const N_PUBLIC: usize = 16;

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
    pub delta_cm: Fr,
    pub epk: (Fr, Fr),
    pub ct: [Fr; 2],
    pub sealer_pk: (Fr, Fr),
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
            self.delta_cm,
            self.epk.0,
            self.epk.1,
            self.ct[0],
            self.ct[1],
            self.sealer_pk.0,
            self.sealer_pk.1,
        ]
    }
}

#[derive(Clone, Debug)]
pub struct SwapCircuit {
    pub public: SwapPublic,
    pub inputs: [InputWitness; 2],
    pub change: Note,
    pub swap: SwapRecord,
    /// Randomness of the delta commitment.
    pub delta_r: Fr,
    /// Ephemeral scalar of the sealing.
    pub eph: JubjubScalar,
    pub sealer_pk: Point,
}

impl SwapCircuit {
    pub fn blank() -> Self {
        let dummy = InputWitness::dummy(0, Fr::zero());
        let swap = SwapRecord {
            epoch: 0,
            pair_id: 0,
            direction: 1,
            delta: 1,
            claim_pk: Fr::zero(),
            rseed: Fr::zero(),
        };
        Self::new(
            Fr::zero(),
            0,
            [dummy.clone(), dummy],
            Note::dummy(0, Fr::zero(), Fr::zero()),
            swap,
            SealerKey::from_seed(b"blank").pk,
            Fr::zero(),
            sealing::scalar_from_seed(&[1u8; 32]),
        )
    }

    /// Seals the amount and builds the public values.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        root: Fr,
        asset_id: u32,
        inputs: [InputWitness; 2],
        change: Note,
        swap: SwapRecord,
        sealer_pk: Point,
        delta_r: Fr,
        eph: JubjubScalar,
    ) -> Self {
        let sealed = sealing::seal(&sealer_pk, swap.delta, delta_r, eph);
        let public = SwapPublic {
            root,
            asset_id,
            nullifiers: [inputs[0].nullifier(), inputs[1].nullifier()],
            change_commitment: change.commitment(),
            swap_commitment: swap.commitment(),
            epoch: swap.epoch,
            pair_id: swap.pair_id,
            direction: swap.direction,
            delta_cm: sealed.delta_cm,
            epk: sealing::point_xy(&sealed.epk),
            ct: sealed.ct,
            sealer_pk: sealing::point_xy(&sealer_pk),
        };
        SwapCircuit {
            public,
            inputs,
            change,
            swap,
            delta_r,
            eph,
            sealer_pk,
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
        let delta_cm_pub = FpVar::new_input(cs.clone(), || Ok(p.delta_cm))?;
        let epk_x_pub = FpVar::new_input(cs.clone(), || Ok(p.epk.0))?;
        let epk_y_pub = FpVar::new_input(cs.clone(), || Ok(p.epk.1))?;
        let ct_pub = [
            FpVar::new_input(cs.clone(), || Ok(p.ct[0]))?,
            FpVar::new_input(cs.clone(), || Ok(p.ct[1]))?,
        ];
        let pk_x = FpVar::new_input(cs.clone(), || Ok(p.sealer_pk.0))?;
        let pk_y = FpVar::new_input(cs.clone(), || Ok(p.sealer_pk.1))?;

        // Spend two inputs, create the change note.
        let mut sum_in = FpVar::<Fr>::zero();
        for (inp, nf_expected) in self.inputs.iter().zip(nf_pub.iter()) {
            sum_in += spend_input(cs.clone(), inp, &asset_id, &root, nf_expected)?;
        }
        let change = output_note(cs.clone(), &self.change, &asset_id, &cm_change)?;

        // The swap record, with a private delta.
        let _ = direction_is_1(&direction)?;
        let delta = FpVar::new_witness(cs.clone(), || Ok(Fr::from(self.swap.delta)))?;
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
        sum_in.enforce_equal(&(change + &delta))?;

        // Verifiable encryption of (delta, r) to the registered sealer key.
        let pk = EdwardsVar::new(pk_x, pk_y);
        let eph_bits: Vec<Boolean<Fr>> = sealing::scalar_bits_le(&self.eph)
            .into_iter()
            .map(|b| Boolean::new_witness(cs.clone(), || Ok(b)))
            .collect::<Result<_, _>>()?;
        let r = FpVar::new_witness(cs.clone(), || Ok(self.delta_r))?;
        let sealed = seal_var(cs, &pk, &eph_bits, &delta, &r)?;
        sealed.epk.x.enforce_equal(&epk_x_pub)?;
        sealed.epk.y.enforce_equal(&epk_y_pub)?;
        sealed.ct[0].enforce_equal(&ct_pub[0])?;
        sealed.ct[1].enforce_equal(&ct_pub[1])?;
        sealed.delta_cm.enforce_equal(&delta_cm_pub)?;
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
        let sealer = SealerKey::from_seed(b"test sealer");
        SwapCircuit::new(
            tree::root_of(&leaves),
            1,
            inputs,
            change,
            swap,
            sealer.pk,
            Fr::from(55u64),
            sealing::scalar_from_seed(&[2u8; 32]),
        )
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
    fn sealer_can_open_what_the_proof_committed() {
        let c = sample();
        let sealer = SealerKey::from_seed(b"test sealer");
        let epk = sealing::point_from_xy(c.public.epk.0, c.public.epk.1).unwrap();
        assert_eq!(
            sealing::unseal(&sealer, &epk, &c.public.ct, c.public.delta_cm),
            Some((100, Fr::from(55u64)))
        );
    }

    #[test]
    fn wrong_commitment_is_rejected() {
        let mut c = sample();
        c.public.delta_cm = Fr::from(1u64);
        let cs = ConstraintSystem::<Fr>::new_ref();
        c.generate_constraints(cs.clone()).unwrap();
        assert!(!cs.is_satisfied().unwrap());
    }

    #[test]
    fn ciphertext_for_another_key_is_rejected() {
        let mut c = sample();
        let other = SealerKey::from_seed(b"other");
        c.public.sealer_pk = sealing::point_xy(&other.pk);
        let cs = ConstraintSystem::<Fr>::new_ref();
        c.generate_constraints(cs.clone()).unwrap();
        assert!(!cs.is_satisfied().unwrap());
    }
}
