//! Sealed swap amounts: verifiable encryption of `(delta, r)` to the sealer's
//! key, so the chain only ever sees a commitment and a ciphertext while the
//! sealer (a TEE or a registered executor) learns the amounts and publishes
//! batch totals.
//!
//! ```text
//! sealer key   sk ∈ Jubjub scalars, PK = sk·G
//! ephemeral    e,  EPK = e·G,  S = e·PK = sk·EPK
//! key          k  = H(DOM_SEALKEY, S.x, S.y)
//! ciphertext   ct = [delta + H(DOM_SEAL, k, 0), r + H(DOM_SEAL, k, 1)]
//! commitment   delta_cm = H(DOM_DELTA, delta, r)
//! ```
//! Jubjub's base field is BLS12-381's scalar field, so every coordinate is a
//! circuit-native element and the whole relation is proven inside the swap
//! circuit. A ciphertext that verifies therefore always opens to the
//! committed amount.

use crate::{poseidon::hash, Fr};
use ark_ec::{AffineRepr, CurveGroup, PrimeGroup};
use ark_ed_on_bls12_381::{EdwardsAffine, EdwardsProjective, Fr as JubjubScalar};
use ark_ff::{BigInteger, PrimeField};
use sha2::{Digest, Sha256};

pub type Point = EdwardsAffine;

pub const DOM_DELTA: u64 = 8;
pub const DOM_SEALKEY: u64 = 9;
pub const DOM_SEAL: u64 = 10;

/// Number of scalar bits fed to the in-circuit scalar multiplications.
pub const SCALAR_BITS: usize = 252;

#[derive(Clone, Debug)]
pub struct SealerKey {
    pub sk: JubjubScalar,
    pub pk: Point,
}

impl SealerKey {
    /// Deterministic from any secret bytes (hashed, then reduced).
    pub fn from_seed(seed: &[u8]) -> Self {
        let digest = Sha256::digest([b"antumbra-sealer-v1".as_slice(), seed].concat());
        let sk = JubjubScalar::from_le_bytes_mod_order(&digest);
        SealerKey {
            sk,
            pk: (EdwardsProjective::generator() * sk).into_affine(),
        }
    }

    pub fn pk_xy(&self) -> (Fr, Fr) {
        point_xy(&self.pk)
    }
}

pub fn point_xy(p: &Point) -> (Fr, Fr) {
    let (x, y) = p.xy().expect("sealing never uses the identity");
    (x, y)
}

/// Rebuilds a point from coordinates, checking the curve equation and the prime-order subgroup.
pub fn point_from_xy(x: Fr, y: Fr) -> Option<Point> {
    let p = Point::new_unchecked(x, y);
    if p.is_zero() || !p.is_on_curve() || !p.is_in_correct_subgroup_assuming_on_curve() {
        return None;
    }
    Some(p)
}

pub fn delta_commitment(delta: u128, r: Fr) -> Fr {
    hash(&[Fr::from(DOM_DELTA), Fr::from(delta), r])
}

fn shared_key(s: &Point) -> Fr {
    let (x, y) = point_xy(s);
    hash(&[Fr::from(DOM_SEALKEY), x, y])
}

fn pad(key: Fr, i: u64) -> Fr {
    hash(&[Fr::from(DOM_SEAL), key, Fr::from(i)])
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Sealed {
    pub epk: Point,
    pub ct: [Fr; 2],
    pub delta_cm: Fr,
}

/// Encrypts `(delta, r)` to `pk` with the ephemeral scalar `eph`.
pub fn seal(pk: &Point, delta: u128, r: Fr, eph: JubjubScalar) -> Sealed {
    let epk = (EdwardsProjective::generator() * eph).into_affine();
    let s = (EdwardsProjective::from(*pk) * eph).into_affine();
    let key = shared_key(&s);
    Sealed {
        epk,
        ct: [Fr::from(delta) + pad(key, 0), r + pad(key, 1)],
        delta_cm: delta_commitment(delta, r),
    }
}

/// Decrypts and checks the commitment; `None` means the entry is inconsistent.
pub fn unseal(key: &SealerKey, epk: &Point, ct: &[Fr; 2], delta_cm: Fr) -> Option<(u128, Fr)> {
    let s = (EdwardsProjective::from(*epk) * key.sk).into_affine();
    let k = shared_key(&s);
    let delta_fr = ct[0] - pad(k, 0);
    let r = ct[1] - pad(k, 1);
    let delta = fr_to_u128(&delta_fr)?;
    (delta_commitment(delta, r) == delta_cm).then_some((delta, r))
}

/// Little-endian bits of a Jubjub scalar, `SCALAR_BITS` long.
pub fn scalar_bits_le(s: &JubjubScalar) -> Vec<bool> {
    let mut bits = s.into_bigint().to_bits_le();
    bits.truncate(SCALAR_BITS);
    while bits.len() < SCALAR_BITS {
        bits.push(false);
    }
    bits
}

pub fn scalar_from_seed(seed: &[u8; 32]) -> JubjubScalar {
    JubjubScalar::from_le_bytes_mod_order(seed)
}

fn fr_to_u128(x: &Fr) -> Option<u128> {
    let bytes = x.into_bigint().to_bytes_le();
    if bytes[16..].iter().any(|b| *b != 0) {
        return None;
    }
    Some(u128::from_le_bytes(bytes[..16].try_into().ok()?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seal_roundtrip_and_tamper() {
        let sealer = SealerKey::from_seed(b"test sealer");
        let eph = scalar_from_seed(&[7u8; 32]);
        let r = Fr::from(123456u64);
        let sealed = seal(&sealer.pk, 100_000, r, eph);
        assert_eq!(
            unseal(&sealer, &sealed.epk, &sealed.ct, sealed.delta_cm),
            Some((100_000, r))
        );
        // Wrong commitment, wrong key, tampered ciphertext all fail.
        assert_eq!(
            unseal(&sealer, &sealed.epk, &sealed.ct, Fr::from(1u64)),
            None
        );
        let other = SealerKey::from_seed(b"other");
        assert_eq!(
            unseal(&other, &sealed.epk, &sealed.ct, sealed.delta_cm),
            None
        );
        let mut ct = sealed.ct;
        ct[0] += Fr::from(1u64);
        assert_eq!(unseal(&sealer, &sealed.epk, &ct, sealed.delta_cm), None);
        // Coordinates round-trip through validation.
        let (x, y) = sealer.pk_xy();
        assert_eq!(point_from_xy(x, y), Some(sealer.pk));
        assert_eq!(point_from_xy(x, y + Fr::from(1u64)), None);
    }
}

/// One sealed swap as the contract stores it and the sealer receives it.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct SealedEntry {
    pub direction: u8,
    pub delta_cm: [u8; 32],
    pub epk_x: [u8; 32],
    pub epk_y: [u8; 32],
    pub ct0: [u8; 32],
    pub ct1: [u8; 32],
}

impl SealedEntry {
    pub const LEN: usize = 1 + 5 * 32;

    pub fn to_bytes(&self) -> [u8; Self::LEN] {
        let mut out = [0u8; Self::LEN];
        out[0] = self.direction;
        out[1..33].copy_from_slice(&self.delta_cm);
        out[33..65].copy_from_slice(&self.epk_x);
        out[65..97].copy_from_slice(&self.epk_y);
        out[97..129].copy_from_slice(&self.ct0);
        out[129..161].copy_from_slice(&self.ct1);
        out
    }
}

/// sha256 over the entries in order: what binds the sealer's totals to one exact batch.
pub fn entries_hash(entries: &[SealedEntry]) -> [u8; 32] {
    let mut h = Sha256::new();
    for e in entries {
        h.update(e.to_bytes());
    }
    h.finalize().into()
}

/// The message a signed executor signs: domain, pool, epoch, pair, entries hash, totals.
pub fn totals_message(
    pool: &str,
    epoch: u64,
    pair_id: u32,
    entries_hash: &[u8; 32],
    delta_1: u128,
    delta_2: u128,
) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(b"antumbra-sealed-totals-v1");
    h.update((pool.len() as u32).to_le_bytes());
    h.update(pool.as_bytes());
    h.update(epoch.to_le_bytes());
    h.update(pair_id.to_le_bytes());
    h.update(entries_hash);
    h.update(delta_1.to_le_bytes());
    h.update(delta_2.to_le_bytes());
    h.finalize().into()
}
