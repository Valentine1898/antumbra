//! Fixture generator for the NEAR gas spike.
//!
//! One toy Groth16 circuit (two private inputs, four public outputs), proved on
//! BN254, BLS12-381 and BLS12-377. For the two curves NEAR has host functions
//! for, the proof and verifying key are also written in the exact byte layout
//! the host functions expect, so the contract does no conversion on-chain.

use std::{fs, path::PathBuf};

use ark_ec::{pairing::Pairing, AffineRepr};
use ark_ff::{BigInteger, PrimeField};
use ark_groth16::{Groth16, PreparedVerifyingKey, Proof, VerifyingKey};
use ark_r1cs_std::{fields::fp::FpVar, prelude::*};
use ark_relations::gr1cs::{ConstraintSynthesizer, ConstraintSystemRef, SynthesisError};
use ark_serialize::CanonicalSerialize;
use ark_snark::SNARK;
use ark_std::rand::{rngs::StdRng, SeedableRng};
use serde_json::{json, Value};

/// Public inputs, in allocation order: c = a*b, d = a+b, e = a*a, f = b*b.
#[derive(Clone)]
struct ToyCircuit<F: PrimeField> {
    a: Option<F>,
    b: Option<F>,
}

impl<F: PrimeField> ToyCircuit<F> {
    fn public_inputs(&self) -> Vec<F> {
        let (a, b) = (self.a.unwrap(), self.b.unwrap());
        vec![a * b, a + b, a * a, b * b]
    }
}

impl<F: PrimeField> ConstraintSynthesizer<F> for ToyCircuit<F> {
    fn generate_constraints(self, cs: ConstraintSystemRef<F>) -> Result<(), SynthesisError> {
        let (wa, wb) = (self.a, self.b);
        let a = FpVar::new_witness(cs.clone(), || wa.ok_or(SynthesisError::AssignmentMissing))?;
        let b = FpVar::new_witness(cs.clone(), || wb.ok_or(SynthesisError::AssignmentMissing))?;
        let want = |f: fn(F, F) -> F| {
            move || {
                let x = wa.ok_or(SynthesisError::AssignmentMissing)?;
                let y = wb.ok_or(SynthesisError::AssignmentMissing)?;
                Ok(f(x, y))
            }
        };
        let c = FpVar::new_input(cs.clone(), want(|x, y| x * y))?;
        let d = FpVar::new_input(cs.clone(), want(|x, y| x + y))?;
        let e = FpVar::new_input(cs.clone(), want(|x, _| x * x))?;
        let f = FpVar::new_input(cs.clone(), want(|_, y| y * y))?;
        (&a * &b).enforce_equal(&c)?;
        (&a + &b).enforce_equal(&d)?;
        (&a * &a).enforce_equal(&e)?;
        (&b * &b).enforce_equal(&f)?;
        Ok(())
    }
}

struct Bundle<E: Pairing> {
    vk: VerifyingKey<E>,
    pvk: PreparedVerifyingKey<E>,
    proof: Proof<E>,
    inputs: Vec<E::ScalarField>,
}

fn setup_and_prove<E: Pairing>() -> Bundle<E> {
    let mut rng = StdRng::seed_from_u64(42);
    let blank = ToyCircuit::<E::ScalarField> { a: None, b: None };
    let (pk, vk) = Groth16::<E>::circuit_specific_setup(blank, &mut rng).expect("setup");
    let circuit = ToyCircuit {
        a: Some(E::ScalarField::from(7u64)),
        b: Some(E::ScalarField::from(11u64)),
    };
    let inputs = circuit.public_inputs();
    let proof = Groth16::<E>::prove(&pk, circuit, &mut rng).expect("prove");
    let pvk = Groth16::<E>::process_vk(&vk).expect("process vk");
    assert!(
        Groth16::<E>::verify_with_processed_vk(&pvk, &inputs, &proof).expect("verify"),
        "proof must verify natively before we ship it as a fixture"
    );
    Bundle {
        vk,
        pvk,
        proof,
        inputs,
    }
}

// ---------- byte helpers ----------

fn le_fixed<F: PrimeField>(x: &F, len: usize) -> Vec<u8> {
    let mut v = x.into_bigint().to_bytes_le();
    assert!(v.len() <= len);
    v.resize(len, 0);
    v
}

fn be_fixed<F: PrimeField>(x: &F, len: usize) -> Vec<u8> {
    let v = x.into_bigint().to_bytes_be();
    let v: Vec<u8> = v.into_iter().skip_while(|b| *b == 0).collect();
    assert!(v.len() <= len);
    let mut out = vec![0u8; len - v.len()];
    out.extend(v);
    out
}

fn ser<T: CanonicalSerialize>(t: &T) -> Vec<u8> {
    let mut v = Vec::new();
    t.serialize_uncompressed(&mut v).expect("serialize");
    v
}

/// NEAR alt_bn128 layout: Fq as 32-byte little-endian, G1 = x||y (64), G2 = x.c0||x.c1||y.c0||y.c1 (128).
mod bn {
    use super::*;
    pub fn g1(p: &ark_bn254::G1Affine) -> Vec<u8> {
        assert!(!p.is_zero());
        [le_fixed(&p.x, 32), le_fixed(&p.y, 32)].concat()
    }
    pub fn g2(p: &ark_bn254::G2Affine) -> Vec<u8> {
        assert!(!p.is_zero());
        [
            le_fixed(&p.x.c0, 32),
            le_fixed(&p.x.c1, 32),
            le_fixed(&p.y.c0, 32),
            le_fixed(&p.y.c1, 32),
        ]
        .concat()
    }
}

/// NEP-488 layout: Fp as 48-byte big-endian, G1 = x||y (96), Fp2 = c1||c0, G2 = x||y (192).
mod bls {
    use super::*;
    pub fn g1(p: &ark_bls12_381::G1Affine) -> Vec<u8> {
        assert!(!p.is_zero());
        [be_fixed(&p.x, 48), be_fixed(&p.y, 48)].concat()
    }
    pub fn g2(p: &ark_bls12_381::G2Affine) -> Vec<u8> {
        assert!(!p.is_zero());
        [
            be_fixed(&p.x.c1, 48),
            be_fixed(&p.x.c0, 48),
            be_fixed(&p.y.c1, 48),
            be_fixed(&p.y.c0, 48),
        ]
        .concat()
    }
}

fn near_blob<E: Pairing>(
    b: &Bundle<E>,
    g1: impl Fn(&E::G1Affine) -> Vec<u8>,
    g2: impl Fn(&E::G2Affine) -> Vec<u8>,
) -> Value {
    let mut vk = Vec::new();
    vk.extend(g1(&b.vk.alpha_g1));
    vk.extend(g2(&b.vk.beta_g2));
    vk.extend(g2(&b.vk.gamma_g2));
    vk.extend(g2(&b.vk.delta_g2));
    for p in &b.vk.gamma_abc_g1 {
        vk.extend(g1(p));
    }
    let proof = [g1(&b.proof.a), g2(&b.proof.b), g1(&b.proof.c)].concat();
    let inputs: Vec<u8> = b.inputs.iter().flat_map(|x| le_fixed(x, 32)).collect();
    json!({
        "vk": hex::encode(vk),
        "proof": hex::encode(proof),
        "inputs": hex::encode(inputs),
        "n_inputs": b.inputs.len(),
    })
}

fn ark_blob<E: Pairing>(b: &Bundle<E>) -> Value {
    json!({
        "vk": hex::encode(ser(&b.vk)),
        "pvk": hex::encode(ser(&b.pvk)),
        "proof": hex::encode(ser(&b.proof)),
        "inputs": hex::encode(ser(&b.inputs)),
    })
}

fn write(dir: &PathBuf, name: &str, v: Value) {
    let path = dir.join(format!("{name}.json"));
    fs::write(&path, serde_json::to_string_pretty(&v).unwrap()).expect("write fixture");
    println!("wrote {}", path.display());
}

fn main() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../fixtures");
    fs::create_dir_all(&dir).unwrap();

    let b = setup_and_prove::<ark_bn254::Bn254>();
    write(
        &dir,
        "bn254",
        json!({ "curve": "bn254", "near": near_blob(&b, bn::g1, bn::g2), "ark": ark_blob(&b) }),
    );

    let b = setup_and_prove::<ark_bls12_381::Bls12_381>();
    write(
        &dir,
        "bls12_381",
        json!({ "curve": "bls12_381", "near": near_blob(&b, bls::g1, bls::g2), "ark": ark_blob(&b) }),
    );

    let b = setup_and_prove::<ark_bls12_377::Bls12_377>();
    write(
        &dir,
        "bls12_377",
        json!({ "curve": "bls12_377", "near": Value::Null, "ark": ark_blob(&b) }),
    );
}
