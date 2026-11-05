//! R1CS mirrors of `poseidon::hash` and the tree, over BLS12-381 Fr.

use crate::{
    poseidon,
    sealing::{DOM_DELTA, DOM_SEAL, DOM_SEALKEY},
    Fr,
};
use ark_crypto_primitives::sponge::{
    constraints::CryptographicSpongeVar, poseidon::constraints::PoseidonSpongeVar,
};
use ark_ec::PrimeGroup;
use ark_ed_on_bls12_381::{constraints::EdwardsVar, EdwardsProjective};
use ark_r1cs_std::{
    boolean::Boolean, fields::fp::FpVar, groups::CurveVar, select::CondSelectGadget,
};
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

/// Sealed output of the swap circuit: `(EPK, ct, delta_cm)`.
pub struct SealedVar {
    pub epk: EdwardsVar,
    pub ct: [FpVar<Fr>; 2],
    pub delta_cm: FpVar<Fr>,
}

/// In-circuit mirror of `sealing::seal`: ECDH on Jubjub with the sealer key
/// `pk`, a Poseidon-derived pad, and the delta commitment.
pub fn seal_var(
    cs: ConstraintSystemRef<Fr>,
    pk: &EdwardsVar,
    eph_bits: &[Boolean<Fr>],
    delta: &FpVar<Fr>,
    r: &FpVar<Fr>,
) -> Result<SealedVar, SynthesisError> {
    let generator = EdwardsVar::constant(EdwardsProjective::generator());
    let epk = generator.scalar_mul_le(eph_bits.iter())?;
    let shared = pk.scalar_mul_le(eph_bits.iter())?;
    let key = hash_var(
        cs.clone(),
        &[domain(DOM_SEALKEY), shared.x.clone(), shared.y.clone()],
    )?;
    let pad0 = hash_var(
        cs.clone(),
        &[
            domain(DOM_SEAL),
            key.clone(),
            FpVar::Constant(Fr::from(0u64)),
        ],
    )?;
    let pad1 = hash_var(
        cs.clone(),
        &[domain(DOM_SEAL), key, FpVar::Constant(Fr::from(1u64))],
    )?;
    let delta_cm = hash_var(cs, &[domain(DOM_DELTA), delta.clone(), r.clone()])?;
    Ok(SealedVar {
        epk,
        ct: [delta + &pad0, r + &pad1],
        delta_cm,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{poseidon::hash, sealing, tree, TREE_DEPTH};
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
    fn gadget_seal_matches_native() {
        let sealer = sealing::SealerKey::from_seed(b"gadget test");
        let eph = sealing::scalar_from_seed(&[3u8; 32]);
        let r = Fr::from(77u64);
        let native = sealing::seal(&sealer.pk, 4242, r, eph);
        let cs = ConstraintSystem::<Fr>::new_ref();
        let pk =
            EdwardsVar::new_input(cs.clone(), || Ok(EdwardsProjective::from(sealer.pk))).unwrap();
        let bits: Vec<Boolean<Fr>> = sealing::scalar_bits_le(&eph)
            .into_iter()
            .map(|b| Boolean::new_witness(cs.clone(), || Ok(b)).unwrap())
            .collect();
        let delta = FpVar::new_witness(cs.clone(), || Ok(Fr::from(4242u64))).unwrap();
        let rv = FpVar::new_witness(cs.clone(), || Ok(r)).unwrap();
        let before = cs.num_constraints();
        let out = seal_var(cs.clone(), &pk, &bits, &delta, &rv).unwrap();
        println!("seal gadget constraints: {}", cs.num_constraints() - before);
        let (ex, ey) = sealing::point_xy(&native.epk);
        assert_eq!(out.epk.x.value().unwrap(), ex);
        assert_eq!(out.epk.y.value().unwrap(), ey);
        assert_eq!(out.ct[0].value().unwrap(), native.ct[0]);
        assert_eq!(out.ct[1].value().unwrap(), native.ct[1]);
        assert_eq!(out.delta_cm.value().unwrap(), native.delta_cm);
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
