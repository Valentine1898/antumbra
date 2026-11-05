//! Antumbra core: the primitives shared by the contract, the circuits and the wallet.
//!
//! Everything here runs natively and inside the NEAR wasm runtime. The R1CS
//! gadgets (feature `r1cs`) mirror the native functions one to one, so a
//! commitment computed by the wallet is the one the circuit recomputes and the
//! one the contract inserts into the tree.

pub use ark_bls12_381::Fr;

pub mod amm;
pub mod consts;
pub mod encoding;
pub mod note;
pub mod poseidon;
pub mod sealing;
pub mod tree;

#[cfg(feature = "crypto")]
pub mod crypto;

#[cfg(feature = "r1cs")]
pub mod gadgets;

/// Depth of the state commitment tree: 2^20 leaves.
pub const TREE_DEPTH: usize = 20;
