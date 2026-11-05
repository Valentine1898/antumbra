//! JoinSplit: 2 inputs, 2 outputs, one asset.
//!
//! Public inputs, in this order:
//! `root, asset_id, nf_1, nf_2, cm_out_1, cm_out_2, public_amount, ext_hash`
//!
//! Statement: both input notes exist in the tree under `root` (unless their
//! amount is zero), their nullifiers are `nf_i`, the output commitments are
//! `cm_out_j`, and `amount_in_1 + amount_in_2 + public_amount = amount_out_1 +
//! amount_out_2` with every amount below 2^128. `ext_hash` is only bound.

use antumbra_core::{
    gadgets::{domain, hash_var, merkle_root_var},
    note::{Note, SpendKey, DOM_NF, DOM_NK, DOM_NOTE, DOM_PK},
    Fr, TREE_DEPTH,
};
use ark_ff::Zero;
use ark_r1cs_std::{
    alloc::AllocVar, boolean::Boolean, convert::ToBitsGadget, eq::EqGadget, fields::fp::FpVar,
    fields::FieldVar,
};
use ark_relations::gr1cs::{ConstraintSynthesizer, ConstraintSystemRef, SynthesisError};

pub const N_PUBLIC: usize = 8;

#[derive(Clone, Debug)]
pub struct InputWitness {
    pub note: Note,
    pub sk: SpendKey,
    pub position: u64,
    pub siblings: Vec<Fr>,
}

impl InputWitness {
    /// Zero-value input; its membership is not checked. `entropy` must be
    /// fresh per transaction: the contract records every nullifier, including
    /// a dummy's, so a repeated dummy would be rejected as a double spend.
    pub fn dummy(asset_id: u32, entropy: Fr) -> Self {
        let sk = SpendKey(entropy);
        InputWitness {
            note: Note::dummy(asset_id, sk.pk(), entropy),
            sk,
            position: 0,
            siblings: vec![Fr::zero(); TREE_DEPTH],
        }
    }
    pub fn nullifier(&self) -> Fr {
        self.note.nullifier(self.sk.nk(), self.position)
    }
}

#[derive(Clone, Debug)]
pub struct JoinSplitPublic {
    pub root: Fr,
    pub asset_id: u32,
    pub nullifiers: [Fr; 2],
    pub commitments: [Fr; 2],
    pub public_amount: Fr,
    pub ext_hash: Fr,
}

impl JoinSplitPublic {
    pub fn to_inputs(&self) -> Vec<Fr> {
        vec![
            self.root,
            Fr::from(self.asset_id as u64),
            self.nullifiers[0],
            self.nullifiers[1],
            self.commitments[0],
            self.commitments[1],
            self.public_amount,
            self.ext_hash,
        ]
    }
}

#[derive(Clone, Debug)]
pub struct JoinSplitCircuit {
    pub public: JoinSplitPublic,
    pub inputs: [InputWitness; 2],
    pub outputs: [Note; 2],
}

impl JoinSplitCircuit {
    /// Shape-only instance for key generation.
    pub fn blank() -> Self {
        let dummy = InputWitness::dummy(0, Fr::zero());
        JoinSplitCircuit {
            public: JoinSplitPublic {
                root: Fr::zero(),
                asset_id: 0,
                nullifiers: [Fr::zero(); 2],
                commitments: [Fr::zero(); 2],
                public_amount: Fr::zero(),
                ext_hash: Fr::zero(),
            },
            inputs: [dummy.clone(), dummy],
            outputs: [Note::dummy(0, Fr::zero(), Fr::zero()); 2],
        }
    }

    /// Builds the public values from the witness (root given explicitly).
    pub fn new(
        root: Fr,
        asset_id: u32,
        inputs: [InputWitness; 2],
        outputs: [Note; 2],
        public_amount: Fr,
        ext_hash: Fr,
    ) -> Self {
        let public = JoinSplitPublic {
            root,
            asset_id,
            nullifiers: [inputs[0].nullifier(), inputs[1].nullifier()],
            commitments: [outputs[0].commitment(), outputs[1].commitment()],
            public_amount,
            ext_hash,
        };
        JoinSplitCircuit {
            public,
            inputs,
            outputs,
        }
    }
}

fn range_check_128(x: &FpVar<Fr>) -> Result<(), SynthesisError> {
    let bits = x.to_bits_le()?;
    for b in &bits[128..] {
        b.enforce_equal(&Boolean::FALSE)?;
    }
    Ok(())
}

impl ConstraintSynthesizer<Fr> for JoinSplitCircuit {
    fn generate_constraints(self, cs: ConstraintSystemRef<Fr>) -> Result<(), SynthesisError> {
        let p = &self.public;
        let root = FpVar::new_input(cs.clone(), || Ok(p.root))?;
        let asset_id = FpVar::new_input(cs.clone(), || Ok(Fr::from(p.asset_id as u64)))?;
        let nf_pub = [
            FpVar::new_input(cs.clone(), || Ok(p.nullifiers[0]))?,
            FpVar::new_input(cs.clone(), || Ok(p.nullifiers[1]))?,
        ];
        let cm_pub = [
            FpVar::new_input(cs.clone(), || Ok(p.commitments[0]))?,
            FpVar::new_input(cs.clone(), || Ok(p.commitments[1]))?,
        ];
        let public_amount = FpVar::new_input(cs.clone(), || Ok(p.public_amount))?;
        let ext_hash = FpVar::new_input(cs.clone(), || Ok(p.ext_hash))?;

        let mut sum_in = FpVar::<Fr>::zero();
        for (inp, nf_expected) in self.inputs.iter().zip(nf_pub.iter()) {
            let amount = FpVar::new_witness(cs.clone(), || Ok(Fr::from(inp.note.amount)))?;
            range_check_128(&amount)?;
            let sk = FpVar::new_witness(cs.clone(), || Ok(inp.sk.0))?;
            let rseed = FpVar::new_witness(cs.clone(), || Ok(inp.note.rseed))?;
            let position_bits: Vec<Boolean<Fr>> = (0..TREE_DEPTH)
                .map(|i| Boolean::new_witness(cs.clone(), || Ok((inp.position >> i) & 1 == 1)))
                .collect::<Result<_, _>>()?;
            let position = Boolean::le_bits_to_fp(&position_bits)?;
            let siblings: Vec<FpVar<Fr>> = inp
                .siblings
                .iter()
                .map(|s| FpVar::new_witness(cs.clone(), || Ok(*s)))
                .collect::<Result<_, _>>()?;

            let pk = hash_var(cs.clone(), &[domain(DOM_PK), sk.clone()])?;
            let nk = hash_var(cs.clone(), &[domain(DOM_NK), sk])?;
            let cm = hash_var(
                cs.clone(),
                &[
                    domain(DOM_NOTE),
                    asset_id.clone(),
                    amount.clone(),
                    pk,
                    rseed,
                ],
            )?;
            let nf = hash_var(cs.clone(), &[domain(DOM_NF), nk, cm.clone(), position])?;
            nf.enforce_equal(nf_expected)?;

            let computed_root = merkle_root_var(cs.clone(), cm, &position_bits, &siblings)?;
            let is_real = !amount.is_zero()?;
            computed_root.conditional_enforce_equal(&root, &is_real)?;

            sum_in += amount;
        }

        let mut sum_out = FpVar::<Fr>::zero();
        for (out, cm_expected) in self.outputs.iter().zip(cm_pub.iter()) {
            let amount = FpVar::new_witness(cs.clone(), || Ok(Fr::from(out.amount)))?;
            range_check_128(&amount)?;
            let pk = FpVar::new_witness(cs.clone(), || Ok(out.pk))?;
            let rseed = FpVar::new_witness(cs.clone(), || Ok(out.rseed))?;
            let cm = hash_var(
                cs.clone(),
                &[
                    domain(DOM_NOTE),
                    asset_id.clone(),
                    amount.clone(),
                    pk,
                    rseed,
                ],
            )?;
            cm.enforce_equal(cm_expected)?;
            sum_out += amount;
        }

        (sum_in + public_amount).enforce_equal(&sum_out)?;

        // Bind ext_hash to the proof even though no constraint needs it.
        let _ = ext_hash.square()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use antumbra_core::{note::withdraw_ext_hash, tree};
    use ark_relations::gr1cs::ConstraintSystem;

    fn sample() -> JoinSplitCircuit {
        let alice = SpendKey(Fr::from(11u64));
        let bob = SpendKey(Fr::from(22u64));
        let a = Note {
            asset_id: 1,
            amount: 600,
            pk: alice.pk(),
            rseed: Fr::from(1u64),
        };
        let other = Note {
            asset_id: 1,
            amount: 5,
            pk: bob.pk(),
            rseed: Fr::from(2u64),
        };
        let leaves = vec![other.commitment(), a.commitment()];
        let root = tree::root_of(&leaves);
        let inputs = [
            InputWitness {
                note: a,
                sk: alice,
                position: 1,
                siblings: tree::merkle_path(&leaves, 1),
            },
            InputWitness::dummy(1, Fr::from(77u64)),
        ];
        let outputs = [
            Note {
                asset_id: 1,
                amount: 250,
                pk: bob.pk(),
                rseed: Fr::from(3u64),
            },
            Note {
                asset_id: 1,
                amount: 250,
                pk: alice.pk(),
                rseed: Fr::from(4u64),
            },
        ];
        JoinSplitCircuit::new(
            root,
            1,
            inputs,
            outputs,
            antumbra_core::encoding::negative_amount(100),
            withdraw_ext_hash("bob.near"),
        )
    }

    #[test]
    fn satisfied_and_constraint_count() {
        let cs = ConstraintSystem::<Fr>::new_ref();
        sample().generate_constraints(cs.clone()).unwrap();
        assert!(cs.is_satisfied().unwrap());
        assert_eq!(cs.num_instance_variables(), N_PUBLIC + 1);
        println!("joinsplit constraints: {}", cs.num_constraints());
    }

    #[test]
    fn unbalanced_is_rejected() {
        let mut c = sample();
        c.outputs[0].amount += 1;
        c.public.commitments[0] = c.outputs[0].commitment();
        let cs = ConstraintSystem::<Fr>::new_ref();
        c.generate_constraints(cs.clone()).unwrap();
        assert!(!cs.is_satisfied().unwrap());
    }

    #[test]
    fn wrong_root_is_rejected() {
        let mut c = sample();
        c.public.root = Fr::from(5u64);
        let cs = ConstraintSystem::<Fr>::new_ref();
        c.generate_constraints(cs.clone()).unwrap();
        assert!(!cs.is_satisfied().unwrap());
    }
}
