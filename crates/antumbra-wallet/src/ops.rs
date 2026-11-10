//! Native, network-aware operations for the CLI: fetch, prepare, submit, record.

use crate::{
    chain::{describe, ensure_ok, Chain, EpochInfo},
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
            .call_with_gas(
                "end_epoch_signed",
                json!({
                    "epoch": epoch, "pair_id": pair_id,
                    "delta_1": totals.delta_1.to_string(), "delta_2": totals.delta_2.to_string(),
                    "signature": pure::b64(&sig),
                }),
                30,
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

/// Clears every finished batch of a pair that still waits among the last
/// `lookback` epochs; returns what it cleared.
///
/// Probing costs one view call per epoch, and a public RPC endpoint cuts a
/// client off after a few hundred calls in a row, so a long window is
/// narrowed first. A swap adds two leaves to the tree: where the leaf count
/// stood still, no batch can be waiting. The count from just before the
/// window is read from the chain's history. If nothing was added since, there
/// is nothing to clear; otherwise the scan starts shortly before the first
/// addition.
pub async fn clear_pending(
    chain: &Chain,
    secrets: &SealerSecrets,
    pair_id: u32,
    lookback: u64,
) -> Result<Vec<Cleared>> {
    let now = chain.current_epoch().await?;
    let mut from = now.epoch.saturating_sub(lookback);
    if now.epoch - from > PLAIN_SCAN {
        match growth_since(chain, from, &now).await? {
            Growth::None => return Ok(vec![]),
            Growth::From(epoch) => from = from.max(epoch.saturating_sub(LATE_EPOCHS)),
            Growth::Unknown => {}
        }
    }
    let paced = now.epoch - from > PLAIN_SCAN;
    let mut out = vec![];
    for epoch in from..now.epoch {
        if let Some(b) = chain.get_batch(epoch, pair_id).await? {
            if !b.executed && b.entries > 0 {
                out.push(clear_epoch(chain, secrets, pair_id, epoch).await?);
            }
        }
        if paced {
            tokio::time::sleep(PROBE_PAUSE).await;
        }
    }
    Ok(out)
}

/// A scan no longer than this many epochs simply probes each of them.
const PLAIN_SCAN: u64 = 64;
/// A swap names the epoch it was proved for and may land later, after a slow
/// wallet approval. This many epochs, about twelve minutes, are allowed
/// between the two.
const LATE_EPOCHS: u64 = 60;
/// Pause between the probes of a long scan, to stay under RPC rate limits.
const PROBE_PAUSE: std::time::Duration = std::time::Duration::from_millis(250);

/// What the chain's history says about the note tree since an epoch began.
enum Growth {
    /// No leaf was added.
    None,
    /// The first leaf was added in this epoch or later.
    From(u64),
    /// The node keeps no state that far back.
    Unknown,
}

/// Finds where the tree first grew since `from_epoch` began, by bisecting the
/// leaf count over past blocks: a handful of view calls for any window.
async fn growth_since(chain: &Chain, from_epoch: u64, now: &EpochInfo) -> Result<Growth> {
    // Epoch e covers the blocks e * len .. (e + 1) * len.
    let len = ((now.block_height + now.blocks_left) / (now.epoch + 1)).max(1);
    let count = chain.leaf_count().await?;
    // A view at a block shows what the blocks before it left behind, so the
    // base is read a whole epoch early: that can only widen the scan.
    let mut lo = from_epoch.saturating_sub(1) * len;
    let Some(base) = chain.leaf_count_at(lo).await? else {
        return Ok(Growth::Unknown);
    };
    if base >= count {
        return Ok(Growth::None);
    }
    // The count at `lo` is still `base`; at `hi` it is higher.
    let mut hi = now.block_height;
    while hi > lo && hi - lo > len {
        let mid = lo + (hi - lo) / 2;
        match chain.leaf_count_at(mid).await? {
            Some(c) if c > base => hi = mid,
            Some(_) => lo = mid,
            None => break,
        }
    }
    Ok(Growth::From(lo / len))
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
