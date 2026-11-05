//! Antumbra shielded pool.
//!
//! Public deposits of NEP-141 tokens into notes, private join-split
//! transfers, withdrawals back to NEAR accounts, shielded swaps executed in
//! per-epoch batches at one clearing price, and private claims of the batch
//! result. One Groth16 proof per action, verified with the NEP-488 host
//! functions.

mod groth16;

use antumbra_core::{
    amm,
    encoding::{bytes32_from_hex, fr_from_bytes, fr_to_bytes, negative_amount, string_to_fr},
    note::{Bsod, DIR_1_TO_2, DIR_2_TO_1, DOM_EXT},
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

/// Verifying keys, NEP-488 layout, produced by `antumbra-setup`.
const JOINSPLIT_VK: &[u8] = include_bytes!("../../../keys/joinsplit.vk.near");
const SWAP_VK: &[u8] = include_bytes!("../../../keys/swap.vk.near");
const SWAPCLAIM_VK: &[u8] = include_bytes!("../../../keys/swapclaim.vk.near");

/// Blocks per epoch. Swaps submitted in an epoch are executed together once it ends.
pub const EPOCH_BLOCKS: u64 = 20;
/// Amounts are limited to 112 bits (the circuits' range checks).
pub const MAX_AMOUNT: u128 = (1u128 << 112) - 1;

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
    Pairs,
    Batches,
}

#[near(serializers = [borsh])]
pub struct Leaf {
    pub cm: [u8; 32],
    pub ciphertext: Vec<u8>,
}

#[near(serializers = [borsh])]
pub struct Pair {
    pub asset_1: u32,
    pub asset_2: u32,
    pub reserve_1: u128,
    pub reserve_2: u128,
}

#[near(serializers = [borsh])]
pub struct Batch {
    pub delta_1: u128,
    pub delta_2: u128,
    pub bsod: Option<Bsod>,
    pub bsod_hash: Option<[u8; 32]>,
}

#[near(serializers = [json])]
pub struct LeafView {
    pub position: u64,
    pub cm: String,
    pub ciphertext: Base64VecU8,
}

#[near(serializers = [json])]
pub struct PairView {
    pub pair_id: u32,
    pub asset_1: u32,
    pub asset_2: u32,
    pub reserve_1: U128,
    pub reserve_2: U128,
}

#[near(serializers = [json])]
pub struct BatchView {
    pub epoch: u64,
    pub pair_id: u32,
    pub executed: bool,
    pub delta_1: U128,
    pub delta_2: U128,
    pub lambda_1: U128,
    pub lambda_2: U128,
    pub unfilled_1: U128,
    pub unfilled_2: U128,
    pub bsod_hash: Option<String>,
}

#[near(serializers = [json])]
pub struct EpochView {
    pub epoch: u64,
    pub block_height: u64,
    pub blocks_left: u64,
}

/// `ft_transfer_call` message: either a note deposit or pool liquidity.
#[near(serializers = [json])]
pub struct DepositMsg {
    pub cm: Option<String>,
    pub ciphertext: Option<Base64VecU8>,
    pub liquidity_pair: Option<u32>,
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
    pairs: Vector<Pair>,
    batches: LookupMap<u64, Batch>,
}

fn batch_key(epoch: u64, pair_id: u32) -> u64 {
    require!(epoch < 1 << 32, "epoch too large");
    (epoch << 32) | pair_id as u64
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
            pairs: Vector::new(StorageKey::Pairs),
            batches: LookupMap::new(StorageKey::Batches),
        }
    }

    // ----- admin -----

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

    /// Registers a trading pair of two registered assets. Pair ids start at 1.
    pub fn register_pair(&mut self, asset_1: u32, asset_2: u32) -> u32 {
        require!(env::predecessor_account_id() == self.owner, "owner only");
        require!(asset_1 != asset_2, "assets must differ");
        self.require_asset(asset_1);
        self.require_asset(asset_2);
        self.pairs.push(Pair {
            asset_1,
            asset_2,
            reserve_1: 0,
            reserve_2: 0,
        });
        self.pairs.len()
    }

    // ----- deposits -----

    /// NEP-141 receiver. `msg` is JSON: `{ "cm": <hex32>, "ciphertext": <base64> }`
    /// mints one note; `{ "liquidity_pair": <pair_id> }` adds the tokens to
    /// that pair's reserves.
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
        require!(
            amount.0 > 0 && amount.0 <= MAX_AMOUNT,
            "amount out of range"
        );
        let msg: DepositMsg = near_sdk::serde_json::from_str(&msg)
            .unwrap_or_else(|_| env::panic_str("bad deposit msg"));
        if let Some(pair_id) = msg.liquidity_pair {
            let pair = self
                .pairs
                .get_mut(pair_id.checked_sub(1).unwrap_or(u32::MAX))
                .unwrap_or_else(|| env::panic_str("unknown pair"));
            if asset_id == pair.asset_1 {
                pair.reserve_1 += amount.0;
            } else if asset_id == pair.asset_2 {
                pair.reserve_2 += amount.0;
            } else {
                env::panic_str("token is not part of the pair");
            }
            env::log_str(&format!(
                "liquidity: sender={sender_id} pair={pair_id} asset={asset_id} amount={}",
                amount.0
            ));
            return PromiseOrValue::Value(U128(0));
        }
        let cm = parse_fr(
            msg.cm
                .as_deref()
                .unwrap_or_else(|| env::panic_str("missing cm")),
            "cm",
        );
        let ciphertext = msg
            .ciphertext
            .unwrap_or_else(|| env::panic_str("missing ciphertext"))
            .0;
        let position = self.insert_leaf(cm, ciphertext);
        env::log_str(&format!(
            "deposit: sender={sender_id} asset={asset_id} amount={} position={position}",
            amount.0
        ));
        PromiseOrValue::Value(U128(0))
    }

    // ----- private actions -----

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
        self.require_asset(asset_id);
        let root_bytes = self.require_root(&root);
        let nf = self.require_fresh_nullifiers(&nullifiers);
        let cm = [
            parse_fr(&commitments[0], "commitment"),
            parse_fr(&commitments[1], "commitment"),
        ];

        let (public_amount, ext_hash) = match (&withdraw_amount, &withdraw_to) {
            (None, None) => (Fr::zero(), Fr::zero()),
            (Some(amount), Some(to)) => {
                require!(
                    amount.0 > 0 && amount.0 <= MAX_AMOUNT,
                    "withdraw amount out of range"
                );
                (
                    negative_amount(amount.0),
                    poseidon::hash(&[Fr::from(DOM_EXT), string_to_fr(to.as_str())]),
                )
            }
            _ => env::panic_str("withdraw_amount and withdraw_to go together"),
        };

        let inputs = [
            root_bytes,
            fr_to_bytes(&Fr::from(asset_id as u64)),
            nf[0],
            nf[1],
            fr_to_bytes(&cm[0]),
            fr_to_bytes(&cm[1]),
            fr_to_bytes(&public_amount),
            fr_to_bytes(&ext_hash),
        ];
        verify_or_panic(JOINSPLIT_VK, &proof.0, &inputs);

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

    /// Shielded swap: burns `delta` of `asset_id` (one side of `pair_id`)
    /// into the batch of `epoch`, keeps the change private, and inserts the
    /// swap commitment so it can be claimed once the batch executes. Any
    /// epoch up to the current one is accepted as long as its batch has not
    /// been executed yet, so a swap that lands one epoch late still clears.
    #[allow(clippy::too_many_arguments)]
    pub fn swap(
        &mut self,
        proof: Base64VecU8,
        root: String,
        asset_id: u32,
        nullifiers: Vec<String>,
        change_commitment: String,
        change_ciphertext: Base64VecU8,
        swap_commitment: String,
        swap_ciphertext: Base64VecU8,
        epoch: u64,
        pair_id: u32,
        direction: u8,
        delta: U128,
    ) -> bool {
        require!(nullifiers.len() == 2, "need 2 nullifiers");
        require!(
            epoch <= self.current_epoch_inner(),
            "swap must not target a future epoch"
        );
        require!(
            direction == DIR_1_TO_2 || direction == DIR_2_TO_1,
            "direction must be 1 or 2"
        );
        require!(delta.0 > 0 && delta.0 <= MAX_AMOUNT, "delta out of range");
        let pair = self.pair(pair_id);
        let burned = if direction == DIR_1_TO_2 {
            pair.asset_1
        } else {
            pair.asset_2
        };
        require!(
            asset_id == burned,
            "asset does not match the pair side being sold"
        );
        let root_bytes = self.require_root(&root);
        let nf = self.require_fresh_nullifiers(&nullifiers);
        let cm_change = parse_fr(&change_commitment, "change commitment");
        let scm = parse_fr(&swap_commitment, "swap commitment");

        let inputs = [
            root_bytes,
            fr_to_bytes(&Fr::from(asset_id as u64)),
            nf[0],
            nf[1],
            fr_to_bytes(&cm_change),
            fr_to_bytes(&scm),
            fr_to_bytes(&Fr::from(epoch)),
            fr_to_bytes(&Fr::from(pair_id as u64)),
            fr_to_bytes(&Fr::from(direction as u64)),
            fr_to_bytes(&Fr::from(delta.0)),
        ];
        verify_or_panic(SWAP_VK, &proof.0, &inputs);

        for n in nf {
            self.nullifiers.insert(n);
        }
        let p0 = self.insert_leaf(cm_change, change_ciphertext.0);
        let p1 = self.insert_leaf(scm, swap_ciphertext.0);
        let key = batch_key(epoch, pair_id);
        let batch = self.batches.entry(key).or_insert_with(|| Batch {
            delta_1: 0,
            delta_2: 0,
            bsod: None,
            bsod_hash: None,
        });
        require!(batch.bsod.is_none(), "batch already executed");
        if direction == DIR_1_TO_2 {
            batch.delta_1 += delta.0;
        } else {
            batch.delta_2 += delta.0;
        }
        env::log_str(&format!(
            "swap: epoch={epoch} pair={pair_id} direction={direction} delta={} positions={p0},{p1}",
            delta.0
        ));
        true
    }

    /// Executes a finished epoch's batch for one pair against the pool.
    /// Permissionless; a no-op batch (no swaps) still gets a result so
    /// nothing is left pending.
    pub fn end_epoch(&mut self, epoch: u64, pair_id: u32) -> BatchView {
        self.execute_batch(epoch, pair_id);
        self.get_batch(epoch, pair_id).expect("batch")
    }

    /// Claims the batch result of a swap: mints the bought amount (and any
    /// unfilled refund) as private notes owned by the swap's claim key.
    #[allow(clippy::too_many_arguments)]
    pub fn swap_claim(
        &mut self,
        proof: Base64VecU8,
        root: String,
        nullifier: String,
        commitments: Vec<String>,
        ciphertexts: Vec<Base64VecU8>,
        epoch: u64,
        pair_id: u32,
    ) -> bool {
        require!(
            commitments.len() == 2 && ciphertexts.len() == 2,
            "need 2 commitments, 2 ciphertexts"
        );
        let root_bytes = self.require_root(&root);
        let nf = self.require_fresh_nullifiers(std::slice::from_ref(&nullifier));
        let cm = [
            parse_fr(&commitments[0], "commitment"),
            parse_fr(&commitments[1], "commitment"),
        ];
        if self
            .batches
            .get(&batch_key(epoch, pair_id))
            .and_then(|b| b.bsod_hash)
            .is_none()
        {
            self.execute_batch(epoch, pair_id);
        }
        let bsod_hash = self
            .batches
            .get(&batch_key(epoch, pair_id))
            .and_then(|b| b.bsod_hash)
            .expect("batch executed");

        let inputs = [
            root_bytes,
            nf[0],
            fr_to_bytes(&cm[0]),
            fr_to_bytes(&cm[1]),
            bsod_hash,
        ];
        verify_or_panic(SWAPCLAIM_VK, &proof.0, &inputs);

        self.nullifiers.insert(nf[0]);
        let mut cts = ciphertexts.into_iter();
        let p0 = self.insert_leaf(cm[0], cts.next().unwrap().0);
        let p1 = self.insert_leaf(cm[1], cts.next().unwrap().0);
        env::log_str(&format!(
            "swap_claim: epoch={epoch} pair={pair_id} positions={p0},{p1}"
        ));
        true
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
        asset_id
            .checked_sub(1)
            .and_then(|i| self.tokens.get(i).cloned())
    }

    pub fn get_pair(&self, pair_id: u32) -> Option<PairView> {
        pair_id
            .checked_sub(1)
            .and_then(|i| self.pairs.get(i))
            .map(|p| PairView {
                pair_id,
                asset_1: p.asset_1,
                asset_2: p.asset_2,
                reserve_1: U128(p.reserve_1),
                reserve_2: U128(p.reserve_2),
            })
    }

    pub fn current_epoch(&self) -> EpochView {
        let height = env::block_height();
        EpochView {
            epoch: height / EPOCH_BLOCKS,
            block_height: height,
            blocks_left: EPOCH_BLOCKS - height % EPOCH_BLOCKS,
        }
    }

    pub fn get_batch(&self, epoch: u64, pair_id: u32) -> Option<BatchView> {
        self.batches.get(&batch_key(epoch, pair_id)).map(|b| {
            let z = Bsod {
                epoch,
                pair_id,
                asset_1: 0,
                asset_2: 0,
                delta_1: b.delta_1,
                delta_2: b.delta_2,
                lambda_1: 0,
                lambda_2: 0,
                unfilled_1: 0,
                unfilled_2: 0,
            };
            let d = b.bsod.as_ref().unwrap_or(&z);
            BatchView {
                epoch,
                pair_id,
                executed: b.bsod.is_some(),
                delta_1: U128(d.delta_1),
                delta_2: U128(d.delta_2),
                lambda_1: U128(d.lambda_1),
                lambda_2: U128(d.lambda_2),
                unfilled_1: U128(d.unfilled_1),
                unfilled_2: U128(d.unfilled_2),
                bsod_hash: b.bsod_hash.map(hex::encode),
            }
        })
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

    fn current_epoch_inner(&self) -> u64 {
        env::block_height() / EPOCH_BLOCKS
    }

    fn require_asset(&self, asset_id: u32) {
        require!(
            asset_id >= 1 && asset_id <= self.tokens.len(),
            "unknown asset"
        );
    }

    fn pair(&self, pair_id: u32) -> &Pair {
        pair_id
            .checked_sub(1)
            .and_then(|i| self.pairs.get(i))
            .unwrap_or_else(|| env::panic_str("unknown pair"))
    }

    fn require_root(&self, root: &str) -> [u8; 32] {
        let root_bytes = bytes32_from_hex(root).unwrap_or_else(|| env::panic_str("bad root"));
        require!(self.known_roots.contains(&root_bytes), "unknown root");
        root_bytes
    }

    /// Parses nullifiers, checks they are canonical, distinct and unspent.
    fn require_fresh_nullifiers(&self, nullifiers: &[String]) -> Vec<[u8; 32]> {
        let nf: Vec<[u8; 32]> = nullifiers
            .iter()
            .map(|n| parse_bytes32(n, "nullifier"))
            .collect();
        for (i, n) in nf.iter().enumerate() {
            require!(fr_from_bytes(n).is_some(), "nullifier not canonical");
            require!(!self.nullifiers.contains(n), "note already spent");
            require!(!nf[..i].contains(n), "duplicate nullifier");
        }
        nf
    }

    fn execute_batch(&mut self, epoch: u64, pair_id: u32) {
        require!(epoch < self.current_epoch_inner(), "epoch not finished");
        let key = batch_key(epoch, pair_id);
        let pair_idx = pair_id.checked_sub(1).unwrap_or(u32::MAX);
        let (asset_1, asset_2) = {
            let p = self
                .pairs
                .get(pair_idx)
                .unwrap_or_else(|| env::panic_str("unknown pair"));
            (p.asset_1, p.asset_2)
        };
        let batch = self.batches.entry(key).or_insert_with(|| Batch {
            delta_1: 0,
            delta_2: 0,
            bsod: None,
            bsod_hash: None,
        });
        require!(batch.bsod.is_none(), "batch already executed");
        let (delta_1, delta_2) = (batch.delta_1, batch.delta_2);

        let pair = self.pairs.get_mut(pair_idx).expect("pair");
        // Direction 1 first (asset_1 -> asset_2), then direction 2 against the updated reserves.
        let (lambda_2, r1, r2) = amm::execute(pair.reserve_1, pair.reserve_2, delta_1);
        let unfilled_1 = if lambda_2 == 0 { delta_1 } else { 0 };
        if lambda_2 > 0 {
            pair.reserve_1 = r1;
            pair.reserve_2 = r2;
        }
        let (lambda_1, r2b, r1b) = amm::execute(pair.reserve_2, pair.reserve_1, delta_2);
        let unfilled_2 = if lambda_1 == 0 { delta_2 } else { 0 };
        if lambda_1 > 0 {
            pair.reserve_2 = r2b;
            pair.reserve_1 = r1b;
        }

        let bsod = Bsod {
            epoch,
            pair_id,
            asset_1,
            asset_2,
            delta_1,
            delta_2,
            lambda_1,
            lambda_2,
            unfilled_1,
            unfilled_2,
        };
        let hash = fr_to_bytes(&bsod.hash());
        let batch = self.batches.get_mut(&key).expect("batch");
        batch.bsod = Some(bsod);
        batch.bsod_hash = Some(hash);
        env::log_str(&format!(
            "end_epoch: epoch={epoch} pair={pair_id} delta_1={delta_1} delta_2={delta_2} lambda_1={lambda_1} lambda_2={lambda_2} unfilled_1={unfilled_1} unfilled_2={unfilled_2}"
        ));
    }

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

fn verify_or_panic(vk: &[u8], proof: &[u8], inputs: &[[u8; 32]]) {
    match groth16::verify(vk, proof, inputs) {
        Some(true) => {}
        Some(false) => env::panic_str("invalid proof"),
        None => env::panic_str("malformed proof"),
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

    fn panic_message<T>(r: Result<T, Box<dyn std::any::Any + Send>>) -> String {
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
    fn joinsplit_paths_reach_verification() {
        let (mut pool, root) = setup();
        let msg = panic_message(catch_unwind(AssertUnwindSafe(|| {
            pool.joinsplit(
                Base64VecU8(vec![0u8; 384]),
                root.clone(),
                1,
                vec![h(1), h(2)],
                vec![h(3), h(4)],
                vec![Base64VecU8(vec![]), Base64VecU8(vec![])],
                None,
                None,
            )
        })));
        assert!(msg.contains("proof"), "{msg}");
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
        assert!(msg.contains("proof"), "{msg}");
    }
}
