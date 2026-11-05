//! R1CS mirrors of `poseidon::hash` and the tree, over BLS12-381 Fr.

use crate::{poseidon, Fr};
use ark_crypto_primitives::sponge::{
    constraints::CryptographicSpongeVar, poseidon::constraints::PoseidonSpongeVar,
};
use ark_r1cs_std::{boolean::Boolean, fields::fp::FpVar, select::CondSelectGadget};
use ark_relations::gr1cs::{ConstraintSystemRef, SynthesisError};

/// In-circuit Poseidon sponge hash, same absorb/squeeze pattern as `poseidon::hash`.
pub fn hash_var(
    cs: ConstraintSystemRef<Fr>,
    inputs: &[FpVar<Fr>],
) -> Result<FpVar<Fr>, SynthesisError> {
    let mut sponge = PoseidonSpongeVar::<Fr>::new(cs, poseidon::config());
    sponge.absorb(&inputs)?;
    let out = sponge.squeeze_field_elements(1)?;
    Ok(out[0].clone())
}

pub fn domain(tag: u64) -> FpVar<Fr> {
    FpVar::Constant(Fr::from(tag))
}

/// Walks the path from `leaf` to the root. `position_bits` are little-endian:
/// bit `i` set means the node at level `i` is a right child.
pub fn merkle_root_var(
    cs: ConstraintSystemRef<Fr>,
    leaf: FpVar<Fr>,
    position_bits: &[Boolean<Fr>],
    siblings: &[FpVar<Fr>],
) -> Result<FpVar<Fr>, SynthesisError> {
    assert_eq!(position_bits.len(), siblings.len());
    let mut cur = leaf;
    for (bit, sib) in position_bits.iter().zip(siblings) {
        let left = FpVar::conditionally_select(bit, sib, &cur)?;
        let right = FpVar::conditionally_select(bit, &cur, sib)?;
        cur = hash_var(cs.clone(), &[left, right])?;
    }
    Ok(cur)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{poseidon::hash, tree, TREE_DEPTH};
    use ark_r1cs_std::{alloc::AllocVar, GR1CSVar};
    use ark_relations::gr1cs::ConstraintSystem;

    #[test]
    fn gadget_hash_matches_native() {
        let cs = ConstraintSystem::<Fr>::new_ref();
        let inputs: Vec<Fr> = (1..=5u64).map(Fr::from).collect();
        let vars: Vec<FpVar<Fr>> = inputs
            .iter()
            .map(|x| FpVar::new_witness(cs.clone(), || Ok(*x)).unwrap())
            .collect();
        let out = hash_var(cs.clone(), &vars).unwrap();
        assert_eq!(out.value().unwrap(), hash(&inputs));
        assert!(cs.is_satisfied().unwrap());
    }

    #[test]
    fn gadget_root_matches_native() {
        let leaves: Vec<Fr> = (10..=13u64).map(Fr::from).collect();
        let position = 2u64;
        let siblings = tree::merkle_path(&leaves, position);
        let cs = ConstraintSystem::<Fr>::new_ref();
        let leaf = FpVar::new_witness(cs.clone(), || Ok(leaves[position as usize])).unwrap();
        let bits: Vec<Boolean<Fr>> = (0..TREE_DEPTH)
            .map(|i| Boolean::new_witness(cs.clone(), || Ok((position >> i) & 1 == 1)).unwrap())
            .collect();
        let sibs: Vec<FpVar<Fr>> = siblings
            .iter()
            .map(|s| FpVar::new_witness(cs.clone(), || Ok(*s)).unwrap())
            .collect();
        let root = merkle_root_var(cs.clone(), leaf, &bits, &sibs).unwrap();
        assert_eq!(root.value().unwrap(), tree::root_of(&leaves));
        assert!(cs.is_satisfied().unwrap());
    }
}
