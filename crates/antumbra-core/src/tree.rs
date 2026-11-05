//! Append-only Poseidon Merkle tree of depth `TREE_DEPTH`.
//!
//! `Frontier` is the on-chain representation: one node per level plus the
//! leaf count, so an insert costs `TREE_DEPTH` hashes and constant storage.
//! `merkle_path` rebuilds a witness from the full leaf list (wallet side).

use crate::{
    consts,
    encoding::{fr_from_bytes, fr_to_bytes},
    poseidon::{hash2, hash_with},
    Fr, TREE_DEPTH,
};
use ark_crypto_primitives::sponge::poseidon::PoseidonConfig;
use ark_ff::Zero;
use borsh::{BorshDeserialize, BorshSerialize};
use std::sync::OnceLock;

/// Derives the empty-subtree roots by hashing with the given parameters.
/// Used by `gen-consts` (with freshly generated parameters) and by tests.
pub fn compute_zeros_with(cfg: &PoseidonConfig<Fr>) -> Vec<Fr> {
    let mut z = Vec::with_capacity(TREE_DEPTH + 1);
    z.push(Fr::zero());
    for i in 0..TREE_DEPTH {
        z.push(hash_with(cfg, &[z[i], z[i]]));
    }
    z
}

/// `zeros()[i]` is the root of an empty subtree of height `i` (embedded constants).
pub fn zeros() -> &'static [Fr] {
    static Z: OnceLock<Vec<Fr>> = OnceLock::new();
    Z.get_or_init(|| {
        consts::TREE_ZEROS
            .iter()
            .map(|b| fr_from_bytes(b).expect("embedded constant is canonical"))
            .collect()
    })
}

#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub struct Frontier {
    /// For each level, the left sibling kept for the next right-child insert.
    pub filled: Vec<[u8; 32]>,
    pub next_index: u64,
    pub root: [u8; 32],
}

impl Default for Frontier {
    fn default() -> Self {
        Self::new()
    }
}

impl Frontier {
    pub fn new() -> Self {
        let z = zeros();
        Frontier {
            filled: z[..TREE_DEPTH].iter().map(fr_to_bytes).collect(),
            next_index: 0,
            root: fr_to_bytes(&z[TREE_DEPTH]),
        }
    }

    pub fn root_fr(&self) -> Fr {
        fr_from_bytes(&self.root).expect("stored root is canonical")
    }

    /// Appends a leaf and returns its position.
    pub fn insert(&mut self, leaf: Fr) -> u64 {
        assert!(self.next_index < 1u64 << TREE_DEPTH, "tree is full");
        let z = zeros();
        let position = self.next_index;
        let mut idx = position;
        let mut cur = leaf;
        for level in 0..TREE_DEPTH {
            if idx & 1 == 0 {
                self.filled[level] = fr_to_bytes(&cur);
                cur = hash2(cur, z[level]);
            } else {
                let left = fr_from_bytes(&self.filled[level]).expect("stored node is canonical");
                cur = hash2(left, cur);
            }
            idx >>= 1;
        }
        self.root = fr_to_bytes(&cur);
        self.next_index += 1;
        position
    }
}

/// Sibling nodes from the leaf level up, for `leaves[position]`.
pub fn merkle_path(leaves: &[Fr], position: u64) -> Vec<Fr> {
    assert!((position as usize) < leaves.len());
    let z = zeros();
    let mut siblings = Vec::with_capacity(TREE_DEPTH);
    let mut level: Vec<Fr> = leaves.to_vec();
    let mut idx = position as usize;
    for height in 0..TREE_DEPTH {
        let sib = idx ^ 1;
        siblings.push(if sib < level.len() {
            level[sib]
        } else {
            z[height]
        });
        let mut next = Vec::with_capacity(level.len().div_ceil(2));
        for pair in level.chunks(2) {
            let right = if pair.len() == 2 { pair[1] } else { z[height] };
            next.push(hash2(pair[0], right));
        }
        level = next;
        idx >>= 1;
    }
    siblings
}

/// Root of the tree holding exactly these leaves.
pub fn root_of(leaves: &[Fr]) -> Fr {
    if leaves.is_empty() {
        return zeros()[TREE_DEPTH];
    }
    let z = zeros();
    let mut level: Vec<Fr> = leaves.to_vec();
    for height in 0..TREE_DEPTH {
        let mut next = Vec::with_capacity(level.len().div_ceil(2));
        for pair in level.chunks(2) {
            let right = if pair.len() == 2 { pair[1] } else { z[height] };
            next.push(hash2(pair[0], right));
        }
        level = next;
    }
    level[0]
}

/// Recomputes the root from a leaf and its path, exactly as the circuit does.
pub fn root_from_path(leaf: Fr, position: u64, siblings: &[Fr]) -> Fr {
    let mut cur = leaf;
    for (level, sib) in siblings.iter().enumerate() {
        cur = if (position >> level) & 1 == 0 {
            hash2(cur, *sib)
        } else {
            hash2(*sib, cur)
        };
    }
    cur
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frontier_matches_full_rebuild() {
        let mut f = Frontier::new();
        assert_eq!(f.root_fr(), root_of(&[]));
        let leaves: Vec<Fr> = (1..=5u64).map(Fr::from).collect();
        for (i, l) in leaves.iter().enumerate() {
            assert_eq!(f.insert(*l), i as u64);
            assert_eq!(f.root_fr(), root_of(&leaves[..=i]));
        }
        for pos in 0..leaves.len() as u64 {
            let path = merkle_path(&leaves, pos);
            assert_eq!(
                root_from_path(leaves[pos as usize], pos, &path),
                f.root_fr()
            );
        }
    }

    #[test]
    fn embedded_zeros_match_hashing() {
        assert_eq!(
            zeros(),
            &compute_zeros_with(crate::poseidon::config())[..],
            "run `cargo run -p antumbra-core --bin gen-consts --release`"
        );
    }

    #[test]
    fn frontier_borsh_roundtrip() {
        let mut f = Frontier::new();
        f.insert(Fr::from(42u64));
        let bytes = borsh::to_vec(&f).unwrap();
        let back: Frontier = borsh::from_slice(&bytes).unwrap();
        assert_eq!(back.root, f.root);
        assert_eq!(back.next_index, 1);
    }
}
