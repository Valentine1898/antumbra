//! Antumbra wallet library: everything the CLI, the demo and the browser
//! build need to hold notes, scan the chain, prove and submit actions.
//!
//! `pure` has no network or OS-randomness dependencies and compiles to wasm;
//! `chain` and `ops` (feature `native`) add near-workspaces for the CLI.

pub mod proving;
pub mod pure;
pub mod state;

#[cfg(feature = "native")]
pub mod chain;
#[cfg(feature = "native")]
pub mod ops;
#[cfg(feature = "native")]
pub mod route;

#[cfg(feature = "native")]
pub use chain::Chain;
pub use proving::ProvingKeys;
pub use state::{Address, WalletFile};
