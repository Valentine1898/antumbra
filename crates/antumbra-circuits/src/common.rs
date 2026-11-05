//! Gadget helpers shared by the circuits: spending an input note, creating
//! an output note, amount range checks and key derivation.

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
use ark_relations::gr1cs::{ConstraintSystemRef, SynthesisError};

/// Amounts are limited to 112 bits so that a product of two amounts (used by
/// the claim's pro-rata check) stays below the field modulus.
pub const AMOUNT_BITS: usize = 112;
pub const MAX_AMOUNT: u128 = (1u128 << AMOUNT_BITS) - 1;

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

pub fn range_check_amount(x: &FpVar<Fr>) -> Result<(), SynthesisError> {
    let bits = x.to_bits_le()?;
    for b in &bits[AMOUNT_BITS..] {
        b.enforce_equal(&Boolean::FALSE)?;
    }
    Ok(())
}

/// `(pk, nk)` for a witness spending key.
pub fn keys_from_sk(
    cs: ConstraintSystemRef<Fr>,
    sk: Fr,
) -> Result<(FpVar<Fr>, FpVar<Fr>), SynthesisError> {
    let sk = FpVar::new_witness(cs.clone(), || Ok(sk))?;
    let pk = hash_var(cs.clone(), &[domain(DOM_PK), sk.clone()])?;
    let nk = hash_var(cs, &[domain(DOM_NK), sk])?;
    Ok((pk, nk))
}

pub fn position_bits(
    cs: ConstraintSystemRef<Fr>,
    position: u64,
) -> Result<Vec<Boolean<Fr>>, SynthesisError> {
    (0..TREE_DEPTH)
        .map(|i| Boolean::new_witness(cs.clone(), || Ok((position >> i) & 1 == 1)))
        .collect()
}

pub fn alloc_siblings(
    cs: ConstraintSystemRef<Fr>,
    siblings: &[Fr],
) -> Result<Vec<FpVar<Fr>>, SynthesisError> {
    assert_eq!(siblings.len(), TREE_DEPTH);
    siblings
        .iter()
        .map(|s| FpVar::new_witness(cs.clone(), || Ok(*s)))
        .collect()
}

/// Spends one input note: recomputes its commitment, proves membership under
/// `root` (skipped for a zero amount), and enforces its nullifier. Returns
/// the amount variable.
pub fn spend_input(
    cs: ConstraintSystemRef<Fr>,
    inp: &InputWitness,
    asset_id: &FpVar<Fr>,
    root: &FpVar<Fr>,
    nf_expected: &FpVar<Fr>,
) -> Result<FpVar<Fr>, SynthesisError> {
    let amount = FpVar::new_witness(cs.clone(), || Ok(Fr::from(inp.note.amount)))?;
    range_check_amount(&amount)?;
    let rseed = FpVar::new_witness(cs.clone(), || Ok(inp.note.rseed))?;
    let (pk, nk) = keys_from_sk(cs.clone(), inp.sk.0)?;
    let bits = position_bits(cs.clone(), inp.position)?;
    let position = Boolean::le_bits_to_fp(&bits)?;
    let siblings = alloc_siblings(cs.clone(), &inp.siblings)?;

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

    let computed_root = merkle_root_var(cs, cm, &bits, &siblings)?;
    let is_real = !amount.is_zero()?;
    computed_root.conditional_enforce_equal(root, &is_real)?;
    Ok(amount)
}

/// Creates an output note with a witness owner key and enforces its
/// commitment. Returns the amount variable.
pub fn output_note(
    cs: ConstraintSystemRef<Fr>,
    note: &Note,
    asset_id: &FpVar<Fr>,
    cm_expected: &FpVar<Fr>,
) -> Result<FpVar<Fr>, SynthesisError> {
    let pk = FpVar::new_witness(cs.clone(), || Ok(note.pk))?;
    output_note_to(cs, note, asset_id, &pk, cm_expected)
}

/// Same as `output_note` but with an already-allocated owner key.
pub fn output_note_to(
    cs: ConstraintSystemRef<Fr>,
    note: &Note,
    asset_id: &FpVar<Fr>,
    pk: &FpVar<Fr>,
    cm_expected: &FpVar<Fr>,
) -> Result<FpVar<Fr>, SynthesisError> {
    let amount = FpVar::new_witness(cs.clone(), || Ok(Fr::from(note.amount)))?;
    range_check_amount(&amount)?;
    let rseed = FpVar::new_witness(cs.clone(), || Ok(note.rseed))?;
    let cm = hash_var(
        cs,
        &[
            domain(DOM_NOTE),
            asset_id.clone(),
            amount.clone(),
            pk.clone(),
            rseed,
        ],
    )?;
    cm.enforce_equal(cm_expected)?;
    Ok(amount)
}

/// Enforces `direction ∈ {1, 2}` and returns `direction == 1`.
pub fn direction_is_1(direction: &FpVar<Fr>) -> Result<Boolean<Fr>, SynthesisError> {
    let one = FpVar::Constant(Fr::from(1u64));
    let two = FpVar::Constant(Fr::from(2u64));
    ((direction - &one) * (direction - &two)).enforce_equal(&FpVar::zero())?;
    direction.is_eq(&one)
}
