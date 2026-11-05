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
use x25519_dalek::{PublicKey, StaticSecret};

pub struct EncryptionKey {
    pub secret: StaticSecret,
    pub public: PublicKey,
}

impl EncryptionKey {
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

pub fn encrypt(recipient: &PublicKey, plaintext: &[u8]) -> Vec<u8> {
    let eph = StaticSecret::from(rand::random::<[u8; 32]>());
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

impl NotePlaintext {
    pub const LEN: usize = 4 + 16 + 32;

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut v = Vec::with_capacity(Self::LEN);
        v.extend_from_slice(&self.asset_id.to_le_bytes());
        v.extend_from_slice(&self.amount.to_le_bytes());
        v.extend_from_slice(&fr_to_bytes(&self.rseed));
        v
    }

    pub fn from_bytes(b: &[u8]) -> Option<Self> {
        if b.len() != Self::LEN {
            return None;
        }
        Some(NotePlaintext {
            asset_id: u32::from_le_bytes(b[0..4].try_into().ok()?),
            amount: u128::from_le_bytes(b[4..20].try_into().ok()?),
            rseed: fr_from_bytes(b[20..52].try_into().ok()?)?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_and_wrong_key() {
        let bob = EncryptionKey::random();
        let eve = EncryptionKey::random();
        let pt = NotePlaintext {
            asset_id: 1,
            amount: 5,
            rseed: Fr::from(9u64),
        };
        let blob = encrypt(&bob.public, &pt.to_bytes());
        assert_eq!(
            NotePlaintext::from_bytes(&decrypt(&bob.secret, &blob).unwrap()),
            Some(pt)
        );
        assert!(decrypt(&eve.secret, &blob).is_none());
    }
}
