//! Antumbra shielded pool.
//!
//! Day-2 surface: public deposits of NEP-141 tokens into notes, private
//! join-split transfers, and withdrawals back to NEAR accounts. Swaps and
//! epoch batches come next.

mod groth16;

use antumbra_core::{
    encoding::{bytes32_from_hex, fr_from_bytes, fr_to_bytes, negative_amount, string_to_fr},
    note::DOM_EXT,
    poseidon,
    tree::Frontier,
    Fr,
};
use ark_ff::Zero;
use near_sdk::{
    env, ext_contract,
    json_types::{Base64VecU8, U128},
    near, require,
    store::{LookupMap, LookupSet, Vector},
    AccountId, BorshStorageKey, Gas, NearToken, PanicOnDefault, PromiseOrValue,
};

/// Join-split verifying key, NEP-488 layout, produced by `antumbra-setup`.
const JOINSPLIT_VK: &[u8] = include_bytes!("../../../keys/joinsplit.vk.near");

const GAS_FT_TRANSFER: Gas = Gas::from_tgas(10);

#[ext_contract(ext_ft)]
pub trait FtCore {
    fn ft_transfer(&mut self, receiver_id: AccountId, amount: U128, memo: Option<String>);
}

#[near(serializers = [borsh])]
#[derive(BorshStorageKey)]
enum StorageKey {
    Assets,
    Tokens,
    Roots,
    Nullifiers,
    Leaves,
}

#[near(serializers = [borsh])]
pub struct Leaf {
    pub cm: [u8; 32],
    pub ciphertext: Vec<u8>,
}

#[near(serializers = [json])]
pub struct LeafView {
    pub position: u64,
    pub cm: String,
    pub ciphertext: Base64VecU8,
}

#[near(serializers = [json])]
pub struct DepositMsg {
    pub cm: String,
    pub ciphertext: Base64VecU8,
}

#[near(contract_state)]
#[derive(PanicOnDefault)]
pub struct Pool {
    owner: AccountId,
    assets: LookupMap<AccountId, u32>,
    tokens: Vector<AccountId>,
    frontier: Frontier,
    known_roots: LookupSet<[u8; 32]>,
    nullifiers: LookupSet<[u8; 32]>,
    leaves: Vector<Leaf>,
}

#[near]
impl Pool {
    #[init]
    pub fn new(owner: AccountId) -> Self {
        let frontier = Frontier::new();
        let mut known_roots = LookupSet::new(StorageKey::Roots);
        known_roots.insert(frontier.root);
        Pool {
            owner,
            assets: LookupMap::new(StorageKey::Assets),
            tokens: Vector::new(StorageKey::Tokens),
            frontier,
            known_roots,
            nullifiers: LookupSet::new(StorageKey::Nullifiers),
            leaves: Vector::new(StorageKey::Leaves),
        }
    }

    /// Registers a NEP-141 token. Asset ids start at 1.
    pub fn register_asset(&mut self, token: AccountId) -> u32 {
        require!(env::predecessor_account_id() == self.owner, "owner only");
        if let Some(id) = self.assets.get(&token) {
            return *id;
        }
        self.tokens.push(token.clone());
        let id = self.tokens.len();
        self.assets.insert(token, id);
        id
    }

    /// NEP-141 receiver: a public deposit that mints one note.
    /// `msg` is JSON `{ "cm": <hex32>, "ciphertext": <base64> }`.
    pub fn ft_on_transfer(
        &mut self,
        sender_id: AccountId,
        amount: U128,
        msg: String,
    ) -> PromiseOrValue<U128> {
        let token = env::predecessor_account_id();
        let asset_id = *self
            .assets
            .get(&token)
            .unwrap_or_else(|| env::panic_str("unregistered token"));
        let msg: DepositMsg = near_sdk::serde_json::from_str(&msg)
            .unwrap_or_else(|_| env::panic_str("bad deposit msg"));
        let cm = parse_fr(&msg.cm, "cm");
        let position = self.insert_leaf(cm, msg.ciphertext.0);
        env::log_str(&format!(
            "deposit: sender={sender_id} asset={asset_id} amount={} position={position}",
            amount.0
        ));
        PromiseOrValue::Value(U128(0))
    }

    /// Private transfer, or withdrawal when `withdraw_amount` and `withdraw_to` are set.
    #[allow(clippy::too_many_arguments)]
    pub fn joinsplit(
        &mut self,
        proof: Base64VecU8,
        root: String,
        asset_id: u32,
        nullifiers: Vec<String>,
        commitments: Vec<String>,
        ciphertexts: Vec<Base64VecU8>,
        withdraw_amount: Option<U128>,
        withdraw_to: Option<AccountId>,
    ) -> PromiseOrValue<bool> {
        require!(
            nullifiers.len() == 2 && commitments.len() == 2 && ciphertexts.len() == 2,
            "need 2 nullifiers, 2 commitments, 2 ciphertexts"
        );
        require!(
            asset_id >= 1 && asset_id <= self.tokens.len(),
            "unknown asset"
        );
        let root_bytes = bytes32_from_hex(&root).unwrap_or_else(|| env::panic_str("bad root"));
        require!(self.known_roots.contains(&root_bytes), "unknown root");
        let nf = [
            parse_bytes32(&nullifiers[0], "nullifier"),
            parse_bytes32(&nullifiers[1], "nullifier"),
        ];
        require!(nf[0] != nf[1], "duplicate nullifier");
        for n in &nf {
            require!(fr_from_bytes(n).is_some(), "nullifier not canonical");
            require!(!self.nullifiers.contains(n), "note already spent");
        }
        let cm = [
            parse_fr(&commitments[0], "commitment"),
            parse_fr(&commitments[1], "commitment"),
        ];

        let (public_amount, ext_hash) = match (&withdraw_amount, &withdraw_to) {
            (None, None) => (Fr::zero(), Fr::zero()),
            (Some(amount), Some(to)) => {
                require!(amount.0 > 0, "withdraw amount must be positive");
                (
                    negative_amount(amount.0),
                    poseidon::hash(&[Fr::from(DOM_EXT), string_to_fr(to.as_str())]),
                )
            }
            _ => env::panic_str("withdraw_amount and withdraw_to go together"),
        };

        let inputs: [[u8; 32]; 8] = [
            root_bytes,
            fr_to_bytes(&Fr::from(asset_id as u64)),
            nf[0],
            nf[1],
            fr_to_bytes(&cm[0]),
            fr_to_bytes(&cm[1]),
            fr_to_bytes(&public_amount),
            fr_to_bytes(&ext_hash),
        ];
        match groth16::verify(JOINSPLIT_VK, &proof.0, &inputs) {
            Some(true) => {}
            Some(false) => env::panic_str("invalid proof"),
            None => env::panic_str("malformed proof"),
        }

        for n in nf {
            self.nullifiers.insert(n);
        }
        let mut cts = ciphertexts.into_iter();
        let p0 = self.insert_leaf(cm[0], cts.next().unwrap().0);
        let p1 = self.insert_leaf(cm[1], cts.next().unwrap().0);
        env::log_str(&format!("joinsplit: asset={asset_id} positions={p0},{p1}"));

        match (withdraw_amount, withdraw_to) {
            (Some(amount), Some(to)) => {
                let token = self.tokens.get(asset_id - 1).expect("asset").clone();
                env::log_str(&format!(
                    "withdraw: asset={asset_id} amount={} to={to}",
                    amount.0
                ));
                PromiseOrValue::Promise(
                    ext_ft::ext(token)
                        .with_attached_deposit(NearToken::from_yoctonear(1))
                        .with_static_gas(GAS_FT_TRANSFER)
                        .ft_transfer(to, amount, Some("antumbra withdrawal".to_string())),
                )
            }
            _ => PromiseOrValue::Value(true),
        }
    }

    // ----- views -----

    pub fn get_root(&self) -> String {
        hex::encode(self.frontier.root)
    }

    pub fn is_known_root(&self, root: String) -> bool {
        bytes32_from_hex(&root)
            .map(|r| self.known_roots.contains(&r))
            .unwrap_or(false)
    }

    pub fn is_spent(&self, nullifier: String) -> bool {
        bytes32_from_hex(&nullifier)
            .map(|n| self.nullifiers.contains(&n))
            .unwrap_or(false)
    }

    pub fn leaf_count(&self) -> u64 {
        self.frontier.next_index
    }

    pub fn get_leaves(&self, from: u64, limit: u64) -> Vec<LeafView> {
        let end = (from + limit).min(self.leaves.len() as u64);
        (from..end)
            .map(|i| {
                let leaf = self.leaves.get(i as u32).expect("leaf");
                LeafView {
                    position: i,
                    cm: hex::encode(leaf.cm),
                    ciphertext: Base64VecU8(leaf.ciphertext.clone()),
                }
            })
            .collect()
    }

    pub fn get_asset_id(&self, token: AccountId) -> Option<u32> {
        self.assets.get(&token).copied()
    }

    pub fn get_token(&self, asset_id: u32) -> Option<AccountId> {
        if asset_id == 0 {
            return None;
        }
        self.tokens.get(asset_id - 1).cloned()
    }

    pub fn get_owner(&self) -> AccountId {
        self.owner.clone()
    }

    /// Gas probe: `n` chained Poseidon(2) hashes. Call it as a transaction to read gas burnt.
    pub fn bench_hash(&self, n: u32) -> String {
        let mut acc = Fr::from(1u64);
        for i in 0..n {
            acc = poseidon::hash2(acc, Fr::from(i as u64));
        }
        hex::encode(fr_to_bytes(&acc))
    }

    // ----- internals -----

    fn insert_leaf(&mut self, cm: Fr, ciphertext: Vec<u8>) -> u64 {
        require!(ciphertext.len() <= 1024, "ciphertext too large");
        let position = self.frontier.insert(cm);
        self.known_roots.insert(self.frontier.root);
        self.leaves.push(Leaf {
            cm: fr_to_bytes(&cm),
            ciphertext,
        });
        position
    }
}

fn parse_bytes32(s: &str, what: &str) -> [u8; 32] {
    bytes32_from_hex(s)
        .unwrap_or_else(|| env::panic_str(&format!("bad {what}: expected 64 hex chars")))
}

fn parse_fr(s: &str, what: &str) -> Fr {
    fr_from_bytes(&parse_bytes32(s, what))
        .unwrap_or_else(|| env::panic_str(&format!("bad {what}: not a canonical field element")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use near_sdk::{test_utils::VMContextBuilder, testing_env};
    use std::panic::{catch_unwind, AssertUnwindSafe};

    fn h(n: u8) -> String {
        hex::encode(fr_to_bytes(&Fr::from(n as u64)))
    }

    fn setup() -> (Pool, String) {
        let owner: AccountId = "owner.near".parse().unwrap();
        let token: AccountId = "token.near".parse().unwrap();
        testing_env!(VMContextBuilder::new()
            .predecessor_account_id(owner.clone())
            .build());
        let mut pool = Pool::new(owner);
        assert_eq!(pool.register_asset(token), 1);
        let root = pool.get_root();
        (pool, root)
    }

    fn panic_message(r: Result<PromiseOrValue<bool>, Box<dyn std::any::Any + Send>>) -> String {
        match r {
            Ok(_) => "no panic".to_string(),
            Err(e) => e
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string()))
                .unwrap_or_else(|| "non-string panic".to_string()),
        }
    }

    #[test]
    fn transfer_path_reaches_verification() {
        let (mut pool, root) = setup();
        let msg = panic_message(catch_unwind(AssertUnwindSafe(|| {
            pool.joinsplit(
                Base64VecU8(vec![0u8; 384]),
                root,
                1,
                vec![h(1), h(2)],
                vec![h(3), h(4)],
                vec![Base64VecU8(vec![]), Base64VecU8(vec![])],
                None,
                None,
            )
        })));
        println!("transfer: {msg}");
        assert!(msg.contains("proof"), "{msg}");
    }

    #[test]
    fn withdraw_path_reaches_verification() {
        let (mut pool, root) = setup();
        let msg = panic_message(catch_unwind(AssertUnwindSafe(|| {
            pool.joinsplit(
                Base64VecU8(vec![0u8; 384]),
                root,
                1,
                vec![h(1), h(2)],
                vec![h(3), h(4)],
                vec![Base64VecU8(vec![]), Base64VecU8(vec![])],
                Some(U128(100)),
                Some("bob.near".parse().unwrap()),
            )
        })));
        println!("withdraw: {msg}");
        assert!(msg.contains("proof"), "{msg}");
    }
}
