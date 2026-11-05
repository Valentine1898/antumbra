//! Browser bindings. Every function takes and returns JSON strings so the
//! wallet state lives in the page (localStorage) and the wasm side stays
//! stateless; randomness is passed in as 32-byte seeds from
//! `crypto.getRandomValues`.

use antumbra_wallet::{
    proving::{self, ProvingKeys},
    pure::{self, LeafData, Prepared, Rng},
    Address, WalletFile,
};
use anyhow::{anyhow, Result};
use serde_json::{json, Value};
use wasm_bindgen::prelude::*;

#[wasm_bindgen(start)]
fn start() {
    console_error_panic_hook::set_once();
}

fn js<T>(r: Result<T>) -> Result<T, JsError> {
    r.map_err(|e| JsError::new(&format!("{e:#}")))
}

fn seed(bytes: &[u8]) -> Result<[u8; 32]> {
    bytes
        .try_into()
        .map_err(|_| anyhow!("seed must be 32 bytes"))
}

fn rng(bytes: &[u8]) -> Result<Rng> {
    Ok(pure::rng_from_seed(seed(bytes)?))
}

fn wallet(json_text: &str) -> Result<WalletFile> {
    Ok(serde_json::from_str(json_text)?)
}

fn amount(s: &str) -> Result<u128> {
    s.trim().parse().map_err(|_| anyhow!("bad amount {s:?}"))
}

/// New wallet from 32 random bytes. Returns the wallet JSON.
#[wasm_bindgen]
pub fn wallet_new(seed_bytes: &[u8]) -> Result<String, JsError> {
    js((|| {
        Ok(serde_json::to_string(&WalletFile::generate_from_seed(
            seed(seed_bytes)?,
        ))?)
    })())
}

#[wasm_bindgen]
pub fn wallet_address(wallet_json: &str) -> Result<String, JsError> {
    js((|| Ok(wallet(wallet_json)?.address().encode()))())
}

/// Public view of the wallet for the UI: balances, notes and swaps without secrets.
#[wasm_bindgen]
pub fn wallet_summary(wallet_json: &str) -> Result<String, JsError> {
    js((|| {
        let w = wallet(wallet_json)?;
        let balances: Vec<Value> = w
            .balances()
            .into_iter()
            .map(|(a, v)| json!({"asset_id": a, "amount": v.to_string()}))
            .collect();
        let notes: Vec<Value> = w
            .notes
            .iter()
            .enumerate()
            .map(|(i, n)| json!({"index": i, "asset_id": n.asset_id, "amount": n.amount, "position": n.position, "spent": n.spent}))
            .collect();
        let swaps: Vec<Value> = w
            .swaps
            .iter()
            .enumerate()
            .map(|(i, s)| json!({"index": i, "epoch": s.epoch, "pair_id": s.pair_id, "direction": s.direction, "delta": s.delta, "position": s.position, "claimed": s.claimed}))
            .collect();
        Ok(json!({"address": w.address().encode(), "leaves": w.leaves.len(), "balances": balances, "notes": notes, "swaps": swaps}).to_string())
    })())
}

/// Appends leaves (`[{position, cm, ciphertext}]`, base64 ciphertexts) and trial-decrypts them.
/// Returns `{ wallet, report }`.
#[wasm_bindgen]
pub fn scan(wallet_json: &str, leaves_json: &str) -> Result<String, JsError> {
    js((|| {
        let mut w = wallet(wallet_json)?;
        let leaves: Vec<LeafData> = serde_json::from_str(leaves_json)?;
        let report = pure::scan_leaves(&mut w, &leaves)?;
        Ok(json!({"wallet": serde_json::to_string(&w)?, "report": report}).to_string())
    })())
}

/// `{ notes: [[index, nullifier]], swaps: [[index, nullifier]] }` for everything unspent/unclaimed.
#[wasm_bindgen]
pub fn nullifiers(wallet_json: &str) -> Result<String, JsError> {
    js((|| {
        let w = wallet(wallet_json)?;
        Ok(
            json!({"notes": pure::note_nullifiers(&w), "swaps": pure::swap_nullifiers(&w)})
                .to_string(),
        )
    })())
}

/// Marks notes/swaps (by index) spent/claimed after the chain confirmed it. Returns the wallet.
#[wasm_bindgen]
pub fn apply_spent(
    wallet_json: &str,
    note_idxs_json: &str,
    swap_idxs_json: &str,
) -> Result<String, JsError> {
    js((|| {
        let mut w = wallet(wallet_json)?;
        let notes: Vec<usize> = serde_json::from_str(note_idxs_json)?;
        let swaps: Vec<usize> = serde_json::from_str(swap_idxs_json)?;
        pure::mark_spent_notes(&mut w, &notes);
        pure::mark_claimed_swaps(&mut w, &swaps);
        Ok(serde_json::to_string(&w)?)
    })())
}

/// Applies a prepared call's bookkeeping once its transaction succeeded. Returns the wallet.
#[wasm_bindgen]
pub fn apply_prepared(wallet_json: &str, prepared_json: &str) -> Result<String, JsError> {
    js((|| {
        let mut w = wallet(wallet_json)?;
        let p: Prepared = serde_json::from_str(prepared_json)?;
        pure::mark_spent_notes(&mut w, &p.spent_notes);
        if let Some(i) = p.claimed_swap {
            pure::mark_claimed_swaps(&mut w, &[i]);
        }
        Ok(serde_json::to_string(&w)?)
    })())
}

/// Deposit message for `ft_transfer_call`. Returns `{ msg, commitment }`.
#[wasm_bindgen]
pub fn prepare_deposit(
    wallet_json: &str,
    asset_id: u32,
    amount_str: &str,
    seed_bytes: &[u8],
) -> Result<String, JsError> {
    js((|| {
        let w = wallet(wallet_json)?;
        let (msg, note) =
            pure::prepare_deposit(&w, asset_id, amount(amount_str)?, &mut rng(seed_bytes)?);
        Ok(json!({"msg": msg.to_string(), "commitment": antumbra_core::encoding::fr_to_hex(&note.commitment())}).to_string())
    })())
}

/// Runs a circuit's deterministic setup; returns the serialised proving key (cache it).
#[wasm_bindgen]
pub fn generate_proving_key(name: &str) -> Result<Vec<u8>, JsError> {
    js(proving::generate_bytes(name))
}

#[wasm_bindgen]
pub struct Prover {
    keys: ProvingKeys,
}

#[wasm_bindgen]
impl Prover {
    #[wasm_bindgen(constructor)]
    pub fn new(joinsplit: &[u8], swap: &[u8], swapclaim: &[u8]) -> Result<Prover, JsError> {
        js(ProvingKeys::from_bytes(joinsplit, swap, swapclaim).map(|keys| Prover { keys }))
    }

    /// Prepared `joinsplit` call for a private transfer.
    pub fn prepare_transfer(
        &self,
        wallet_json: &str,
        asset_id: u32,
        amount_str: &str,
        to_address: &str,
        seed_bytes: &[u8],
    ) -> Result<String, JsError> {
        js((|| {
            let w = wallet(wallet_json)?;
            let to = Address::parse(to_address)?;
            let p = pure::prepare_transfer(
                &w,
                &self.keys,
                asset_id,
                amount(amount_str)?,
                to,
                &mut rng(seed_bytes)?,
            )?;
            Ok(serde_json::to_string(&p)?)
        })())
    }

    /// Prepared `joinsplit` call for a withdrawal to a NEAR account.
    pub fn prepare_withdraw(
        &self,
        wallet_json: &str,
        asset_id: u32,
        amount_str: &str,
        to_account: &str,
        seed_bytes: &[u8],
    ) -> Result<String, JsError> {
        js((|| {
            let w = wallet(wallet_json)?;
            let p = pure::prepare_withdraw(
                &w,
                &self.keys,
                asset_id,
                amount(amount_str)?,
                to_account,
                &mut rng(seed_bytes)?,
            )?;
            Ok(serde_json::to_string(&p)?)
        })())
    }

    /// Prepared `swap` call. `epoch` is the contract's current epoch.
    #[allow(clippy::too_many_arguments)]
    pub fn prepare_swap(
        &self,
        wallet_json: &str,
        pair_id: u32,
        asset_1: u32,
        asset_2: u32,
        sell_asset: u32,
        amount_str: &str,
        epoch: f64,
        seed_bytes: &[u8],
    ) -> Result<String, JsError> {
        js((|| {
            let w = wallet(wallet_json)?;
            let p = pure::prepare_swap(
                &w,
                &self.keys,
                pair_id,
                (asset_1, asset_2),
                sell_asset,
                amount(amount_str)?,
                epoch as u64,
                &mut rng(seed_bytes)?,
            )?;
            Ok(serde_json::to_string(&p)?)
        })())
    }

    /// Prepared `swap_claim` call for swap `index`. `batch_json` is the contract's
    /// `get_batch` view. Returns `{ prepared, outputs: [{asset_id, amount}, ...] }`.
    pub fn prepare_claim(
        &self,
        wallet_json: &str,
        index: u32,
        asset_1: u32,
        asset_2: u32,
        batch_json: &str,
        seed_bytes: &[u8],
    ) -> Result<String, JsError> {
        js((|| {
            let w = wallet(wallet_json)?;
            let entry = w
                .swaps
                .get(index as usize)
                .ok_or_else(|| anyhow!("no swap #{index}"))?;
            let batch: Value = serde_json::from_str(batch_json)?;
            let bsod =
                pure::bsod_from_view(entry.epoch, entry.pair_id, (asset_1, asset_2), &batch)?;
            let (p, outputs) =
                pure::prepare_claim(&w, &self.keys, index as usize, &bsod, &mut rng(seed_bytes)?)?;
            let outs: Vec<Value> = outputs
                .iter()
                .map(|n| json!({"asset_id": n.asset_id, "amount": n.amount.to_string()}))
                .collect();
            Ok(json!({"prepared": p, "outputs": outs}).to_string())
        })())
    }
}
