//! Test fungible token: standard NEP-141 core plus an open `mint` so tests can
//! fund accounts, and a `storage_deposit` so accounts can be registered.
#![allow(deprecated)]

use near_contract_standards::{
    fungible_token::FungibleToken,
    storage_management::{StorageBalance, StorageManagement},
};
use near_sdk::{json_types::U128, near, AccountId, PanicOnDefault, PromiseOrValue};

#[near(contract_state)]
#[derive(PanicOnDefault)]
pub struct TestFt {
    token: FungibleToken,
}

#[near]
impl TestFt {
    #[init]
    pub fn new() -> Self {
        TestFt {
            token: FungibleToken::new(b"t".to_vec()),
        }
    }

    pub fn mint(&mut self, account_id: AccountId, amount: U128) {
        if !self.token.accounts.contains_key(&account_id) {
            self.token.internal_register_account(&account_id);
        }
        self.token.internal_deposit(&account_id, amount.0);
    }

    #[payable]
    pub fn storage_deposit(
        &mut self,
        account_id: Option<AccountId>,
        registration_only: Option<bool>,
    ) -> StorageBalance {
        self.token.storage_deposit(account_id, registration_only)
    }
}

near_contract_standards::impl_fungible_token_core!(TestFt, token);
