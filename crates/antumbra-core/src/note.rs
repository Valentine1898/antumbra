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

/// Direction of a swap within a pair: 1 burns asset_1 for asset_2, 2 the reverse.
pub const DIR_1_TO_2: u8 = 1;
pub const DIR_2_TO_1: u8 = 2;

/// A swap intent: `delta` of one side of `pair_id`, in `epoch`, claimable by
/// the owner of `claim_pk`. Its commitment is a leaf of the commitment tree.
///
/// ```text
/// scm = H(DOM_SWAP, epoch, pair_id, direction, delta, claim_pk, rseed)
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SwapRecord {
    pub epoch: u64,
    pub pair_id: u32,
    pub direction: u8,
    pub delta: u128,
    pub claim_pk: Fr,
    pub rseed: Fr,
}

impl SwapRecord {
    pub fn commitment(&self) -> Fr {
        hash(&[
            Fr::from(DOM_SWAP),
            Fr::from(self.epoch),
            Fr::from(self.pair_id as u64),
            Fr::from(self.direction as u64),
            Fr::from(self.delta),
            self.claim_pk,
            self.rseed,
        ])
    }

    pub fn nullifier(&self, nk: Fr, position: u64) -> Fr {
        hash(&[Fr::from(DOM_NF), nk, self.commitment(), Fr::from(position)])
    }
}

/// Batch swap output data: the public result of executing one epoch of one
/// pair, exactly Penumbra's shape plus the pair's asset ids. `lambda_2` pays
/// the `delta_1` side, `lambda_1` the `delta_2` side; `unfilled_*` is the
/// input returned when the pool could not execute.
#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct Bsod {
    pub epoch: u64,
    pub pair_id: u32,
    pub asset_1: u32,
    pub asset_2: u32,
    pub delta_1: u128,
    pub delta_2: u128,
    pub lambda_1: u128,
    pub lambda_2: u128,
    pub unfilled_1: u128,
    pub unfilled_2: u128,
}

impl Bsod {
    pub fn hash(&self) -> Fr {
        hash(&[
            Fr::from(DOM_BSOD),
            Fr::from(self.epoch),
            Fr::from(self.pair_id as u64),
            Fr::from(self.asset_1 as u64),
            Fr::from(self.asset_2 as u64),
            Fr::from(self.delta_1),
            Fr::from(self.delta_2),
            Fr::from(self.lambda_1),
            Fr::from(self.lambda_2),
            Fr::from(self.unfilled_1),
            Fr::from(self.unfilled_2),
        ])
    }

    /// Pro-rata share of a swap of `delta` in `direction`:
    /// `(output amount, refund amount, output asset, refund asset)`.
    pub fn claim_amounts(&self, direction: u8, delta: u128) -> (u128, u128, u32, u32) {
        let (total, lambda, unfilled, asset_out, asset_in) = if direction == DIR_1_TO_2 {
            (
                self.delta_1,
                self.lambda_2,
                self.unfilled_1,
                self.asset_2,
                self.asset_1,
            )
        } else {
            (
                self.delta_2,
                self.lambda_1,
                self.unfilled_2,
                self.asset_1,
                self.asset_2,
            )
        };
        (
            crate::amm::mul_div(delta, lambda, total),
            crate::amm::mul_div(delta, unfilled, total),
            asset_out,
            asset_in,
        )
    }
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
