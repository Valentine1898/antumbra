//! SwapClaim: turn an executed swap into private output notes.
//!
//! Public inputs, in this order: `root, nf_swap, cm_out_1, cm_out_2, bsod_hash`
//!
//! Statement: a swap record `(epoch, pair_id, direction, delta, claim_pk, rseed)`
//! with `claim_pk = H(DOM_PK, sk)` is in the tree under `root`, its nullifier
//! is `nf_swap`, `bsod_hash` commits to the batch result of that epoch and
//! pair, and the two outputs are the pro-rata share:
//! `out = floor(delta * lambda / delta_total)` of the bought asset and
//! `refund = floor(delta * unfilled / delta_total)` of the sold asset, both
//! owned by `claim_pk`.

use crate::common::{
    alloc_siblings, direction_is_1, keys_from_sk, output_note_to, position_bits, range_check_amount,
};
use antumbra_core::{
    amm::mul_div,
    gadgets::{domain, hash_var, merkle_root_var},
    note::{Bsod, Note, SpendKey, SwapRecord, DOM_BSOD, DOM_NF, DOM_SWAP},
    Fr, TREE_DEPTH,
};
use ark_ff::Zero;
use ark_r1cs_std::{
    alloc::AllocVar, boolean::Boolean, eq::EqGadget, fields::fp::FpVar, select::CondSelectGadget,
};
use ark_relations::gr1cs::{ConstraintSynthesizer, ConstraintSystemRef, SynthesisError};
use primitive_types::U256;
use std::cmp::Ordering;

pub const N_PUBLIC: usize = 5;

#[derive(Clone, Debug)]
pub struct SwapClaimPublic {
    pub root: Fr,
    pub nullifier: Fr,
    pub commitments: [Fr; 2],
    pub bsod_hash: Fr,
}

impl SwapClaimPublic {
    pub fn to_inputs(&self) -> Vec<Fr> {
        vec![
            self.root,
            self.nullifier,
            self.commitments[0],
            self.commitments[1],
            self.bsod_hash,
        ]
    }
}

#[derive(Clone, Debug)]
pub struct SwapClaimCircuit {
    pub public: SwapClaimPublic,
    pub swap: SwapRecord,
    pub sk: SpendKey,
    pub position: u64,
    pub siblings: Vec<Fr>,
    pub bsod: Bsod,
    /// `[bought asset note, refund note]`, both owned by `claim_pk`.
    pub outputs: [Note; 2],
}

impl SwapClaimCircuit {
    pub fn blank() -> Self {
        let swap = SwapRecord {
            epoch: 0,
            pair_id: 0,
            direction: 1,
            delta: 0,
            claim_pk: Fr::zero(),
            rseed: Fr::zero(),
        };
        let bsod = Bsod {
            epoch: 0,
            pair_id: 0,
            asset_1: 0,
            asset_2: 0,
            delta_1: 1,
            delta_2: 1,
            lambda_1: 0,
            lambda_2: 0,
            unfilled_1: 0,
            unfilled_2: 0,
        };
        SwapClaimCircuit {
            public: SwapClaimPublic {
                root: Fr::zero(),
                nullifier: Fr::zero(),
                commitments: [Fr::zero(); 2],
                bsod_hash: Fr::zero(),
            },
            swap,
            sk: SpendKey(Fr::zero()),
            position: 0,
            siblings: vec![Fr::zero(); TREE_DEPTH],
            bsod,
            outputs: [Note::dummy(0, Fr::zero(), Fr::zero()); 2],
        }
    }

    /// Computes the outputs from the batch result and builds the public values.
    pub fn new(
        root: Fr,
        swap: SwapRecord,
        sk: SpendKey,
        position: u64,
        siblings: Vec<Fr>,
        bsod: Bsod,
        out_rseeds: [Fr; 2],
    ) -> Self {
        assert_eq!(sk.pk(), swap.claim_pk, "claim key must own the swap");
        assert_eq!(
            (bsod.epoch, bsod.pair_id),
            (swap.epoch, swap.pair_id),
            "batch must match the swap"
        );
        let (out, refund, asset_out, asset_in) = bsod.claim_amounts(swap.direction, swap.delta);
        let outputs = [
            Note {
                asset_id: asset_out,
                amount: out,
                pk: swap.claim_pk,
                rseed: out_rseeds[0],
            },
            Note {
                asset_id: asset_in,
                amount: refund,
                pk: swap.claim_pk,
                rseed: out_rseeds[1],
            },
        ];
        let public = SwapClaimPublic {
            root,
            nullifier: swap.nullifier(sk.nk(), position),
            commitments: [outputs[0].commitment(), outputs[1].commitment()],
            bsod_hash: bsod.hash(),
        };
        SwapClaimCircuit {
            public,
            swap,
            sk,
            position,
            siblings,
            bsod,
            outputs,
        }
    }
}

/// Witness for `q = floor(a * b / c)`: `(q, r)` with `a * b = q * c + r`, `r < c`.
fn floor_div_witness(a: u128, b: u128, c: u128) -> (u128, u128) {
    if c == 0 {
        return (0, 0);
    }
    let q = mul_div(a, b, c);
    let r = U256::from(a) * U256::from(b) - U256::from(q) * U256::from(c);
    (q, r.try_into().expect("remainder below c"))
}

/// Enforces `q = floor(a * b / c)` in-circuit given the native values.
fn floor_div(
    cs: ConstraintSystemRef<Fr>,
    a: &FpVar<Fr>,
    b: &FpVar<Fr>,
    c: &FpVar<Fr>,
    (a_v, b_v, c_v): (u128, u128, u128),
) -> Result<FpVar<Fr>, SynthesisError> {
    let (q_v, r_v) = floor_div_witness(a_v, b_v, c_v);
    let q = FpVar::new_witness(cs.clone(), || Ok(Fr::from(q_v)))?;
    let r = FpVar::new_witness(cs, || Ok(Fr::from(r_v)))?;
    range_check_amount(&q)?;
    range_check_amount(&r)?;
    (a * b).enforce_equal(&(&q * c + &r))?;
    r.enforce_cmp(c, Ordering::Less, false)?;
    Ok(q)
}

impl ConstraintSynthesizer<Fr> for SwapClaimCircuit {
    fn generate_constraints(self, cs: ConstraintSystemRef<Fr>) -> Result<(), SynthesisError> {
        let p = &self.public;
        let root = FpVar::new_input(cs.clone(), || Ok(p.root))?;
        let nf_pub = FpVar::new_input(cs.clone(), || Ok(p.nullifier))?;
        let cm_pub = [
            FpVar::new_input(cs.clone(), || Ok(p.commitments[0]))?,
            FpVar::new_input(cs.clone(), || Ok(p.commitments[1]))?,
        ];
        let bsod_hash_pub = FpVar::new_input(cs.clone(), || Ok(p.bsod_hash))?;

        // The swap record and its membership.
        let s = &self.swap;
        let (pk, nk) = keys_from_sk(cs.clone(), self.sk.0)?;
        let epoch = FpVar::new_witness(cs.clone(), || Ok(Fr::from(s.epoch)))?;
        let pair_id = FpVar::new_witness(cs.clone(), || Ok(Fr::from(s.pair_id as u64)))?;
        let direction = FpVar::new_witness(cs.clone(), || Ok(Fr::from(s.direction as u64)))?;
        let delta = FpVar::new_witness(cs.clone(), || Ok(Fr::from(s.delta)))?;
        let rseed = FpVar::new_witness(cs.clone(), || Ok(s.rseed))?;
        range_check_amount(&delta)?;
        let is_dir1 = direction_is_1(&direction)?;
        let scm = hash_var(
            cs.clone(),
            &[
                domain(DOM_SWAP),
                epoch.clone(),
                pair_id.clone(),
                direction,
                delta.clone(),
                pk.clone(),
                rseed,
            ],
        )?;
        let bits = position_bits(cs.clone(), self.position)?;
        let position = Boolean::le_bits_to_fp(&bits)?;
        let siblings = alloc_siblings(cs.clone(), &self.siblings)?;
        merkle_root_var(cs.clone(), scm.clone(), &bits, &siblings)?.enforce_equal(&root)?;
        hash_var(cs.clone(), &[domain(DOM_NF), nk, scm, position])?.enforce_equal(&nf_pub)?;

        // The batch result, bound by its hash.
        let b = &self.bsod;
        let w = |v: u128| FpVar::new_witness(cs.clone(), || Ok(Fr::from(v)));
        let asset_1 = w(b.asset_1 as u128)?;
        let asset_2 = w(b.asset_2 as u128)?;
        let delta_1 = w(b.delta_1)?;
        let delta_2 = w(b.delta_2)?;
        let lambda_1 = w(b.lambda_1)?;
        let lambda_2 = w(b.lambda_2)?;
        let unfilled_1 = w(b.unfilled_1)?;
        let unfilled_2 = w(b.unfilled_2)?;
        for x in [
            &delta_1,
            &delta_2,
            &lambda_1,
            &lambda_2,
            &unfilled_1,
            &unfilled_2,
        ] {
            range_check_amount(x)?;
        }
        hash_var(
            cs.clone(),
            &[
                domain(DOM_BSOD),
                epoch,
                pair_id,
                asset_1.clone(),
                asset_2.clone(),
                delta_1.clone(),
                delta_2.clone(),
                lambda_1.clone(),
                lambda_2.clone(),
                unfilled_1.clone(),
                unfilled_2.clone(),
            ],
        )?
        .enforce_equal(&bsod_hash_pub)?;

        // Pro-rata share for this swap's direction.
        let total = FpVar::conditionally_select(&is_dir1, &delta_1, &delta_2)?;
        let lambda = FpVar::conditionally_select(&is_dir1, &lambda_2, &lambda_1)?;
        let unfilled = FpVar::conditionally_select(&is_dir1, &unfilled_1, &unfilled_2)?;
        let asset_out = FpVar::conditionally_select(&is_dir1, &asset_2, &asset_1)?;
        let asset_in = FpVar::conditionally_select(&is_dir1, &asset_1, &asset_2)?;
        let (total_v, lambda_v, unfilled_v) = if s.direction == 1 {
            (b.delta_1, b.lambda_2, b.unfilled_1)
        } else {
            (b.delta_2, b.lambda_1, b.unfilled_2)
        };
        let out = floor_div(
            cs.clone(),
            &delta,
            &lambda,
            &total,
            (s.delta, lambda_v, total_v),
        )?;
        let refund = floor_div(
            cs.clone(),
            &delta,
            &unfilled,
            &total,
            (s.delta, unfilled_v, total_v),
        )?;

        // Output notes: amounts must equal the computed shares.
        let out_amt = output_note_to(cs.clone(), &self.outputs[0], &asset_out, &pk, &cm_pub[0])?;
        out_amt.enforce_equal(&out)?;
        let refund_amt = output_note_to(cs, &self.outputs[1], &asset_in, &pk, &cm_pub[1])?;
        refund_amt.enforce_equal(&refund)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use antumbra_core::tree;
    use ark_relations::gr1cs::ConstraintSystem;

    fn sample() -> SwapClaimCircuit {
        let alice = SpendKey(Fr::from(11u64));
        let swap = SwapRecord {
            epoch: 7,
            pair_id: 1,
            direction: 1,
            delta: 100,
            claim_pk: alice.pk(),
            rseed: Fr::from(4u64),
        };
        let leaves = vec![Fr::from(5u64), swap.commitment()];
        // Two swappers in the batch: 100 + 300 sold for 360 bought, nothing unfilled.
        let bsod = Bsod {
            epoch: 7,
            pair_id: 1,
            asset_1: 1,
            asset_2: 2,
            delta_1: 400,
            delta_2: 0,
            lambda_1: 0,
            lambda_2: 360,
            unfilled_1: 0,
            unfilled_2: 0,
        };
        SwapClaimCircuit::new(
            tree::root_of(&leaves),
            swap,
            alice,
            1,
            tree::merkle_path(&leaves, 1),
            bsod,
            [Fr::from(8u64), Fr::from(9u64)],
        )
    }

    #[test]
    fn satisfied_and_amounts() {
        let c = sample();
        assert_eq!(c.outputs[0].amount, 90); // 100 * 360 / 400
        assert_eq!(c.outputs[0].asset_id, 2);
        assert_eq!(c.outputs[1].amount, 0);
        assert_eq!(c.outputs[1].asset_id, 1);
        let cs = ConstraintSystem::<Fr>::new_ref();
        c.generate_constraints(cs.clone()).unwrap();
        assert!(cs.is_satisfied().unwrap());
        assert_eq!(cs.num_instance_variables(), N_PUBLIC + 1);
        println!("swapclaim constraints: {}", cs.num_constraints());
    }

    #[test]
    fn overclaim_is_rejected() {
        let mut c = sample();
        c.outputs[0].amount = 91;
        c.public.commitments[0] = c.outputs[0].commitment();
        let cs = ConstraintSystem::<Fr>::new_ref();
        c.generate_constraints(cs.clone()).unwrap();
        assert!(!cs.is_satisfied().unwrap());
    }

    #[test]
    fn rounding_leaves_remainder() {
        // 7 * 10 / 3 = 23 remainder 1
        assert_eq!(floor_div_witness(7, 10, 3), (23, 1));
    }
}
