//! Notes, keys, commitments and nullifiers.
//!
//! ```text
//! pk  = H(DOM_PK, sk)                       owner key a note is locked to
//! nk  = H(DOM_NK, sk)                       nullifier key
//! cm  = H(DOM_NOTE, asset_id, amount, pk, rseed)
//! nf  = H(DOM_NF, nk, cm, position)
//! ```

use crate::{encoding, poseidon::hash, Fr};

pub const DOM_PK: u64 = 1;
pub const DOM_NK: u64 = 2;
pub const DOM_NOTE: u64 = 3;
pub const DOM_NF: u64 = 4;
pub const DOM_EXT: u64 = 5;
pub const DOM_SWAP: u64 = 6;
pub const DOM_BSOD: u64 = 7;

/// The wallet's spending secret.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpendKey(pub Fr);

impl SpendKey {
    pub fn pk(&self) -> Fr {
        hash(&[Fr::from(DOM_PK), self.0])
    }
    pub fn nk(&self) -> Fr {
        hash(&[Fr::from(DOM_NK), self.0])
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Note {
    pub asset_id: u32,
    pub amount: u128,
    pub pk: Fr,
    pub rseed: Fr,
}

impl Note {
    pub fn commitment(&self) -> Fr {
        hash(&[
            Fr::from(DOM_NOTE),
            Fr::from(self.asset_id as u64),
            Fr::from(self.amount),
            self.pk,
            self.rseed,
        ])
    }

    pub fn nullifier(&self, nk: Fr, position: u64) -> Fr {
        hash(&[Fr::from(DOM_NF), nk, self.commitment(), Fr::from(position)])
    }

    /// A zero-value note. Its Merkle membership is not checked by the circuit;
    /// it only exists so every JoinSplit has exactly two inputs.
    pub fn dummy(asset_id: u32, pk: Fr, rseed: Fr) -> Self {
        Note {
            asset_id,
            amount: 0,
            pk,
            rseed,
        }
    }
}

/// Binding hash for a withdrawal: ties the proof to the recipient account.
pub fn withdraw_ext_hash(recipient: &str) -> Fr {
    hash(&[Fr::from(DOM_EXT), encoding::string_to_fr(recipient)])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commitments_differ_by_every_field() {
        let sk = SpendKey(Fr::from(7u64));
        let base = Note {
            asset_id: 1,
            amount: 10,
            pk: sk.pk(),
            rseed: Fr::from(99u64),
        };
        let cm = base.commitment();
        assert_ne!(cm, Note { amount: 11, ..base }.commitment());
        assert_ne!(
            cm,
            Note {
                asset_id: 2,
                ..base
            }
            .commitment()
        );
        assert_ne!(
            cm,
            Note {
                rseed: Fr::from(100u64),
                ..base
            }
            .commitment()
        );
        assert_ne!(
            cm,
            Note {
                pk: SpendKey(Fr::from(8u64)).pk(),
                ..base
            }
            .commitment()
        );
        assert_ne!(base.nullifier(sk.nk(), 0), base.nullifier(sk.nk(), 1));
    }
}
