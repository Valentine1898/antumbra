//! Antumbra wallet library: everything the CLI (and the demo) needs to hold
//! notes, scan the chain, prove and submit actions.

pub mod chain;
pub mod ops;
pub mod proving;
pub mod state;

pub use chain::Chain;
pub use proving::ProvingKeys;
pub use state::{Address, WalletFile};
