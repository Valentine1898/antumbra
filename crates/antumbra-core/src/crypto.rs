//! Note encryption for delivery: X25519 ECDH with an ephemeral key, then
//! ChaCha20-Poly1305. The wallet scans the on-chain ciphertext log and
//! trial-decrypts every entry with its own secret.
//!
//! blob = ephemeral_pk (32) || aead_ciphertext

use crate::{
    encoding::{fr_from_bytes, fr_to_bytes},
    Fr,
};
use chacha20poly1305::{
    aead::{Aead, KeyInit},
    ChaCha20Poly1305, Key, Nonce,
};
use sha2::{Digest, Sha256};
pub use x25519_dalek::{PublicKey, StaticSecret};

pub struct EncryptionKey {
    pub secret: StaticSecret,
    pub public: PublicKey,
}

impl EncryptionKey {
    #[cfg(feature = "std")]
    pub fn random() -> Self {
        let bytes: [u8; 32] = rand::random();
        Self::from_bytes(bytes)
    }
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        let secret = StaticSecret::from(bytes);
        let public = PublicKey::from(&secret);
        EncryptionKey { secret, public }
    }
}

fn derive(shared: &[u8; 32], ephemeral_pk: &[u8; 32]) -> (Key, Nonce) {
    let mut h = Sha256::new();
    h.update(b"antumbra-note-v1");
    h.update(shared);
    h.update(ephemeral_pk);
    let key = h.finalize();
    let mut n = Sha256::new();
    n.update(b"antumbra-nonce-v1");
    n.update(ephemeral_pk);
    let nonce_full = n.finalize();
    (
        *Key::from_slice(&key),
        *Nonce::from_slice(&nonce_full[..12]),
    )
}

/// Encrypts with a fresh OS-random ephemeral key.
#[cfg(feature = "std")]
pub fn encrypt(recipient: &PublicKey, plaintext: &[u8]) -> Vec<u8> {
    encrypt_with_seed(recipient, plaintext, rand::random())
}

/// Encrypts with the given ephemeral secret; `eph_seed` must be fresh randomness.
pub fn encrypt_with_seed(recipient: &PublicKey, plaintext: &[u8], eph_seed: [u8; 32]) -> Vec<u8> {
    let eph = StaticSecret::from(eph_seed);
    let eph_pk = PublicKey::from(&eph);
    let shared = eph.diffie_hellman(recipient);
    let (key, nonce) = derive(shared.as_bytes(), eph_pk.as_bytes());
    let ct = ChaCha20Poly1305::new(&key)
        .encrypt(&nonce, plaintext)
        .expect("aead");
    let mut out = eph_pk.as_bytes().to_vec();
    out.extend(ct);
    out
}

pub fn decrypt(secret: &StaticSecret, blob: &[u8]) -> Option<Vec<u8>> {
    if blob.len() < 32 + 16 {
        return None;
    }
    let eph_pk_bytes: [u8; 32] = blob[..32].try_into().ok()?;
    let eph_pk = PublicKey::from(eph_pk_bytes);
    let shared = secret.diffie_hellman(&eph_pk);
    let (key, nonce) = derive(shared.as_bytes(), &eph_pk_bytes);
    ChaCha20Poly1305::new(&key)
        .decrypt(&nonce, &blob[32..])
        .ok()
}

/// What the sender tells the recipient about a note. The owner key `pk` is
/// the recipient's own, so it is not transmitted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NotePlaintext {
    pub asset_id: u32,
    pub amount: u128,
    pub rseed: Fr,
}

/// What a swapper stores for their future self: everything needed to rebuild
/// the swap record and claim it (`claim_pk` is the wallet's own key).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SwapPlaintext {
    pub epoch: u64,
    pub pair_id: u32,
    pub direction: u8,
    pub delta: u128,
    pub rseed: Fr,
}

/// Tagged plaintext carried by a ciphertext in the on-chain log.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Plaintext {
    Note(NotePlaintext),
    Swap(SwapPlaintext),
}

const TAG_NOTE: u8 = 0;
const TAG_SWAP: u8 = 1;

impl Plaintext {
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut v = Vec::with_capacity(64);
        match self {
            Plaintext::Note(n) => {
                v.push(TAG_NOTE);
                v.extend_from_slice(&n.asset_id.to_le_bytes());
                v.extend_from_slice(&n.amount.to_le_bytes());
                v.extend_from_slice(&fr_to_bytes(&n.rseed));
            }
            Plaintext::Swap(s) => {
                v.push(TAG_SWAP);
                v.extend_from_slice(&s.epoch.to_le_bytes());
                v.extend_from_slice(&s.pair_id.to_le_bytes());
                v.push(s.direction);
                v.extend_from_slice(&s.delta.to_le_bytes());
                v.extend_from_slice(&fr_to_bytes(&s.rseed));
            }
        }
        v
    }

    pub fn from_bytes(b: &[u8]) -> Option<Self> {
        match *b.first()? {
            TAG_NOTE if b.len() == 1 + 4 + 16 + 32 => Some(Plaintext::Note(NotePlaintext {
                asset_id: u32::from_le_bytes(b[1..5].try_into().ok()?),
                amount: u128::from_le_bytes(b[5..21].try_into().ok()?),
                rseed: fr_from_bytes(b[21..53].try_into().ok()?)?,
            })),
            TAG_SWAP if b.len() == 1 + 8 + 4 + 1 + 16 + 32 => {
                Some(Plaintext::Swap(SwapPlaintext {
                    epoch: u64::from_le_bytes(b[1..9].try_into().ok()?),
                    pair_id: u32::from_le_bytes(b[9..13].try_into().ok()?),
                    direction: b[13],
                    delta: u128::from_le_bytes(b[14..30].try_into().ok()?),
                    rseed: fr_from_bytes(b[30..62].try_into().ok()?)?,
                }))
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_and_wrong_key() {
        let bob = EncryptionKey::random();
        let eve = EncryptionKey::random();
        let pt = Plaintext::Note(NotePlaintext {
            asset_id: 1,
            amount: 5,
            rseed: Fr::from(9u64),
        });
        let blob = encrypt(&bob.public, &pt.to_bytes());
        assert_eq!(
            Plaintext::from_bytes(&decrypt(&bob.secret, &blob).unwrap()),
            Some(pt)
        );
        assert!(decrypt(&eve.secret, &blob).is_none());
        let sw = Plaintext::Swap(SwapPlaintext {
            epoch: 3,
            pair_id: 1,
            direction: 2,
            delta: 77,
            rseed: Fr::from(4u64),
        });
        assert_eq!(Plaintext::from_bytes(&sw.to_bytes()), Some(sw));
        assert_eq!(Plaintext::from_bytes(&[9u8; 53]), None);
    }
}
