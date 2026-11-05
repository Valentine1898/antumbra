//! Native, network-aware operations for the CLI: fetch, prepare, submit, record.

use crate::{
    chain::{describe, ensure_ok, Chain},
    proving::ProvingKeys,
    pure::{self, LeafData, Rng},
    state::{Address, WalletFile},
};
use antumbra_core::encoding::fr_to_hex;
use anyhow::{anyhow, Result};
use near_workspaces::{result::ExecutionFinalResult, AccountId};
use serde_json::json;

fn rng() -> Rng {
    pure::rng_from_seed(rand::random())
}

#[derive(Debug, Default)]
pub struct SyncReport {
    pub new_leaves: u64,
    pub new_notes: usize,
    pub new_swaps: usize,
    pub newly_spent: usize,
    pub newly_claimed: usize,
}

/// Pulls new leaves, trial-decrypts them, and refreshes spent/claimed flags.
pub async fn sync(wallet: &mut WalletFile, chain: &Chain) -> Result<SyncReport> {
    let mut report = SyncReport::default();
    let total = chain.leaf_count().await?;
    let mut from = wallet.leaves.len() as u64;
    while from < total {
        let page = chain.get_leaves(from, 50).await?;
        if page.is_empty() {
            break;
        }
        let leaves: Vec<LeafData> = page
            .into_iter()
            .map(|l| LeafData {
                position: l.position,
                cm: l.cm,
                ciphertext: l.ciphertext,
            })
            .collect();
        let r = pure::scan_leaves(wallet, &leaves)?;
        report.new_leaves += r.new_leaves;
        report.new_notes += r.new_notes;
        report.new_swaps += r.new_swaps;
        from = wallet.leaves.len() as u64;
    }
    let mut spent = vec![];
    for (i, nf) in pure::note_nullifiers(wallet) {
        if chain.is_spent(&nf).await? {
            spent.push(i);
        }
    }
    report.newly_spent = spent.len();
    pure::mark_spent_notes(wallet, &spent);
    let mut claimed = vec![];
    for (i, nf) in pure::swap_nullifiers(wallet) {
        if chain.is_spent(&nf).await? {
            claimed.push(i);
        }
    }
    report.newly_claimed = claimed.len();
    pure::mark_claimed_swaps(wallet, &claimed);
    Ok(report)
}

/// Public deposit of `amount` of `token` into a fresh note owned by this wallet.
pub async fn deposit(
    wallet: &WalletFile,
    chain: &Chain,
    token: &AccountId,
    amount: u128,
) -> Result<ExecutionFinalResult> {
    let asset_id = chain
        .get_asset_id(token)
        .await?
        .ok_or_else(|| anyhow!("token {token} is not registered in the pool"))?;
    let (msg, _) = pure::prepare_deposit(wallet, asset_id, amount, &mut rng());
    ensure_ok(chain.ft_transfer_call(token, amount, msg).await?)
}

async fn submit(
    wallet: &mut WalletFile,
    chain: &Chain,
    p: pure::Prepared,
) -> Result<ExecutionFinalResult> {
    let r = ensure_ok(chain.call(&p.method, p.args).await?)?;
    pure::mark_spent_notes(wallet, &p.spent_notes);
    if let Some(i) = p.claimed_swap {
        pure::mark_claimed_swaps(wallet, &[i]);
    }
    Ok(r)
}

pub async fn transfer(
    wallet: &mut WalletFile,
    chain: &Chain,
    keys: &ProvingKeys,
    asset_id: u32,
    amount: u128,
    to: Address,
) -> Result<ExecutionFinalResult> {
    let p = pure::prepare_transfer(wallet, keys, asset_id, amount, to, &mut rng())?;
    submit(wallet, chain, p).await
}

pub async fn withdraw(
    wallet: &mut WalletFile,
    chain: &Chain,
    keys: &ProvingKeys,
    asset_id: u32,
    amount: u128,
    to: &AccountId,
) -> Result<ExecutionFinalResult> {
    let p = pure::prepare_withdraw(wallet, keys, asset_id, amount, to.as_str(), &mut rng())?;
    submit(wallet, chain, p).await
}

/// Sells `amount` of `sell_asset` on `pair_id` in the current epoch, sealed to the pool's sealer.
pub async fn swap(
    wallet: &mut WalletFile,
    chain: &Chain,
    keys: &ProvingKeys,
    pair_id: u32,
    sell_asset: u32,
    amount: u128,
) -> Result<(u64, ExecutionFinalResult)> {
    let pair = chain
        .get_pair(pair_id)
        .await?
        .ok_or_else(|| anyhow!("unknown pair {pair_id}"))?;
    let sealer = chain
        .get_sealer()
        .await?
        .ok_or_else(|| anyhow!("the pool has no sealer registered yet"))?;
    let epoch = chain.current_epoch().await?.epoch;
    let p = pure::prepare_swap(
        wallet,
        keys,
        pair_id,
        (pair.asset_1, pair.asset_2),
        sell_asset,
        amount,
        epoch,
        (&sealer.pk[0], &sealer.pk[1]),
        &mut rng(),
    )?;
    let r = submit(wallet, chain, p).await?;
    Ok((epoch, r))
}

/// The sealer's secrets for the signed executor path.
pub struct SealerSecrets {
    pub seed: Vec<u8>,
    pub executor: ed25519_dalek::SigningKey,
}

impl SealerSecrets {
    pub fn sealer_key(&self) -> antumbra_core::sealing::SealerKey {
        antumbra_core::sealing::SealerKey::from_seed(&self.seed)
    }
    /// `(pk_x hex, pk_y hex, executor pk hex)` for `set_sealer`.
    pub fn public_parts(&self) -> (String, String, String) {
        let (x, y) = self.sealer_key().pk_xy();
        (
            fr_to_hex(&x),
            fr_to_hex(&y),
            hex::encode(self.executor.verifying_key().to_bytes()),
        )
    }
}

#[derive(Debug)]
pub struct Cleared {
    pub epoch: u64,
    pub entries: usize,
    pub delta_1: u128,
    pub delta_2: u128,
    pub summary: String,
}

/// Runs the sealer on a finished epoch and submits the signed totals.
pub async fn clear_epoch(
    chain: &Chain,
    secrets: &SealerSecrets,
    pair_id: u32,
    epoch: u64,
) -> Result<Cleared> {
    let entries_json = chain.get_batch_entries(epoch, pair_id).await?;
    let entries: Vec<antumbra_core::sealing::SealedEntry> = entries_json
        .iter()
        .map(|e| e.to_entry())
        .collect::<Result<_>>()?;
    let totals = antumbra_sealer::clear(&secrets.sealer_key(), epoch, pair_id, &entries);
    if !totals.invalid.is_empty() {
        return Err(anyhow!(
            "entries {:?} did not open to their commitments",
            totals.invalid
        ));
    }
    let msg = antumbra_core::sealing::totals_message(
        chain.pool.as_str(),
        epoch,
        pair_id,
        &totals.entries_hash,
        totals.delta_1,
        totals.delta_2,
    );
    use ed25519_dalek::Signer;
    let sig = secrets.executor.sign(&msg).to_bytes();
    let r = ensure_ok(
        chain
            .call(
                "end_epoch_signed",
                json!({
                    "epoch": epoch, "pair_id": pair_id,
                    "delta_1": totals.delta_1.to_string(), "delta_2": totals.delta_2.to_string(),
                    "signature": pure::b64(&sig),
                }),
            )
            .await?,
    )?;
    Ok(Cleared {
        epoch,
        entries: entries.len(),
        delta_1: totals.delta_1,
        delta_2: totals.delta_2,
        summary: describe(&r),
    })
}

/// Clears every finished epoch of a pair that still has an open batch; returns what it cleared.
pub async fn clear_pending(
    chain: &Chain,
    secrets: &SealerSecrets,
    pair_id: u32,
    lookback: u64,
) -> Result<Vec<Cleared>> {
    let now = chain.current_epoch().await?.epoch;
    let mut out = vec![];
    for epoch in now.saturating_sub(lookback)..now {
        if let Some(b) = chain.get_batch(epoch, pair_id).await? {
            if !b.executed && b.entries > 0 {
                out.push(clear_epoch(chain, secrets, pair_id, epoch).await?);
            }
        }
    }
    Ok(out)
}

#[derive(Debug)]
pub enum ClaimOutcome {
    Claimed {
        index: usize,
        bought_asset: u32,
        bought: u128,
        refund: u128,
        summary: String,
    },
    Waiting {
        index: usize,
        epoch: u64,
        blocks_left: u64,
    },
    /// The epoch has ended but the sealer has not cleared the batch yet.
    NotCleared { index: usize, epoch: u64 },
}

/// Claims every claimable swap, executing finished batches on the way.
pub async fn claim_all(
    wallet: &mut WalletFile,
    chain: &Chain,
    keys: &ProvingKeys,
) -> Result<Vec<ClaimOutcome>> {
    let mut outcomes = Vec::new();
    let now = chain.current_epoch().await?;
    for i in 0..wallet.swaps.len() {
        if wallet.swaps[i].claimed {
            continue;
        }
        let entry = wallet.swaps[i].clone();
        if entry.epoch >= now.epoch {
            let blocks_left = (entry.epoch - now.epoch) * 20 + now.blocks_left;
            outcomes.push(ClaimOutcome::Waiting {
                index: i,
                epoch: entry.epoch,
                blocks_left,
            });
            continue;
        }
        let batch = chain.get_batch(entry.epoch, entry.pair_id).await?;
        let Some(b) = batch.filter(|b| b.executed) else {
            outcomes.push(ClaimOutcome::NotCleared {
                index: i,
                epoch: entry.epoch,
            });
            continue;
        };
        let pair = chain
            .get_pair(entry.pair_id)
            .await?
            .ok_or_else(|| anyhow!("unknown pair"))?;
        let bsod = pure::bsod_from_view(
            entry.epoch,
            entry.pair_id,
            (pair.asset_1, pair.asset_2),
            &serde_json::to_value(&b)?,
        )?;
        let (p, outputs) = pure::prepare_claim(wallet, keys, i, &bsod, &mut rng())?;
        let r = submit(wallet, chain, p).await?;
        outcomes.push(ClaimOutcome::Claimed {
            index: i,
            bought_asset: outputs[0].asset_id,
            bought: outputs[0].amount,
            refund: outputs[1].amount,
            summary: describe(&r),
        });
    }
    Ok(outcomes)
}
