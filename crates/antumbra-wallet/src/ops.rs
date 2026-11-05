//! User-level operations: sync, deposit, transfer, withdraw, swap, claim.

use crate::{
    chain::{describe, ensure_ok, Chain},
    proving::ProvingKeys,
    state::{Address, NoteEntry, SwapEntry, WalletFile},
};
use antumbra_circuits::{
    common::InputWitness, joinsplit::JoinSplitCircuit, keys, near_encoding, swap::SwapCircuit,
    swapclaim::SwapClaimCircuit,
};
use antumbra_core::{
    crypto::{self, NotePlaintext, Plaintext, SwapPlaintext},
    encoding::{fr_from_hex, fr_to_hex, negative_amount},
    note::{withdraw_ext_hash, Bsod, Note, SwapRecord, DIR_1_TO_2, DIR_2_TO_1},
    tree, Fr,
};
use anyhow::{anyhow, Result};
use ark_ff::{UniformRand, Zero};
use ark_std::rand::{rngs::StdRng, SeedableRng};
use base64::Engine;
use near_workspaces::{result::ExecutionFinalResult, AccountId};
use serde_json::json;

fn random_fr() -> Fr {
    Fr::rand(&mut StdRng::from_seed(rand::random()))
}

fn b64(bytes: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

fn seal(to: &Address, pt: Plaintext) -> Vec<u8> {
    crypto::encrypt(&to.public_key(), &pt.to_bytes())
}

fn note_plaintext(n: &Note) -> Plaintext {
    Plaintext::Note(NotePlaintext {
        asset_id: n.asset_id,
        amount: n.amount,
        rseed: n.rseed,
    })
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
    let me = wallet.address();
    let enc = wallet.enc_key();
    let pk = me.pk;
    while from < total {
        let page = chain.get_leaves(from, 50).await?;
        if page.is_empty() {
            break;
        }
        for leaf in page {
            if leaf.position != wallet.leaves.len() as u64 {
                return Err(anyhow!(
                    "leaf log out of order at position {}",
                    leaf.position
                ));
            }
            wallet.leaves.push(leaf.cm.clone());
            report.new_leaves += 1;
            let Some(cm) = fr_from_hex(&leaf.cm) else {
                continue;
            };
            let Ok(blob) = base64::engine::general_purpose::STANDARD.decode(&leaf.ciphertext)
            else {
                continue;
            };
            let Some(pt) =
                crypto::decrypt(&enc.secret, &blob).and_then(|b| Plaintext::from_bytes(&b))
            else {
                continue;
            };
            match pt {
                Plaintext::Note(n) => {
                    let note = Note {
                        asset_id: n.asset_id,
                        amount: n.amount,
                        pk,
                        rseed: n.rseed,
                    };
                    // Zero-value notes (withdrawal padding) are ours but useless; skip them.
                    if n.amount > 0 && note.commitment() == cm {
                        wallet.notes.push(NoteEntry {
                            asset_id: n.asset_id,
                            amount: n.amount.to_string(),
                            rseed: fr_to_hex(&n.rseed),
                            position: leaf.position,
                            spent: false,
                        });
                        report.new_notes += 1;
                    }
                }
                Plaintext::Swap(s) => {
                    let swap = SwapRecord {
                        epoch: s.epoch,
                        pair_id: s.pair_id,
                        direction: s.direction,
                        delta: s.delta,
                        claim_pk: pk,
                        rseed: s.rseed,
                    };
                    if swap.commitment() == cm {
                        wallet.swaps.push(SwapEntry {
                            epoch: s.epoch,
                            pair_id: s.pair_id,
                            direction: s.direction,
                            delta: s.delta.to_string(),
                            rseed: fr_to_hex(&s.rseed),
                            position: leaf.position,
                            claimed: false,
                        });
                        report.new_swaps += 1;
                    }
                }
            }
        }
        from = wallet.leaves.len() as u64;
    }
    // Refresh spent flags against the chain (a note spent from another device, or a failed local mark).
    let nk = wallet.spend_key().nk();
    for i in 0..wallet.notes.len() {
        if wallet.notes[i].spent {
            continue;
        }
        let nf = wallet
            .note(&wallet.notes[i])
            .nullifier(nk, wallet.notes[i].position);
        if chain.is_spent(&fr_to_hex(&nf)).await? {
            wallet.notes[i].spent = true;
            report.newly_spent += 1;
        }
    }
    for i in 0..wallet.swaps.len() {
        if wallet.swaps[i].claimed {
            continue;
        }
        let nf = wallet
            .swap(&wallet.swaps[i])
            .nullifier(nk, wallet.swaps[i].position);
        if chain.is_spent(&fr_to_hex(&nf)).await? {
            wallet.swaps[i].claimed = true;
            report.newly_claimed += 1;
        }
    }
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
    let me = wallet.address();
    let note = Note {
        asset_id,
        amount,
        pk: me.pk,
        rseed: random_fr(),
    };
    let msg = json!({"cm": fr_to_hex(&note.commitment()), "ciphertext": b64(&seal(&me, note_plaintext(&note)))});
    ensure_ok(chain.ft_transfer_call(token, amount, msg).await?)
}

/// Builds the two input witnesses for the selected notes (second may be a dummy).
fn inputs_for(
    wallet: &WalletFile,
    idxs: &[usize],
    asset_id: u32,
) -> Result<([InputWitness; 2], u128)> {
    let leaves = wallet.leaves_fr();
    let sk = wallet.spend_key();
    let mut total = 0u128;
    let mut witnesses = Vec::new();
    for &i in idxs {
        let e = &wallet.notes[i];
        let note = wallet.note(e);
        if e.position as usize >= leaves.len() {
            return Err(anyhow!("wallet is behind the chain, run `sync` first"));
        }
        total += note.amount;
        witnesses.push(InputWitness {
            note,
            sk,
            position: e.position,
            siblings: tree::merkle_path(&leaves, e.position),
        });
    }
    while witnesses.len() < 2 {
        witnesses.push(InputWitness::dummy(asset_id, random_fr()));
    }
    let arr: [InputWitness; 2] = witnesses
        .try_into()
        .map_err(|_| anyhow!("two inputs expected"))?;
    Ok((arr, total))
}

#[allow(clippy::too_many_arguments)]
async fn submit_joinsplit(
    wallet: &mut WalletFile,
    chain: &Chain,
    keys: &ProvingKeys,
    asset_id: u32,
    idxs: Vec<usize>,
    outputs: [Note; 2],
    ciphertexts: [Vec<u8>; 2],
    withdraw: Option<(u128, &AccountId)>,
) -> Result<ExecutionFinalResult> {
    let (inputs, _) = inputs_for(wallet, &idxs, asset_id)?;
    let root = tree::root_of(&wallet.leaves_fr());
    let (public_amount, ext_hash) = match withdraw {
        Some((amount, to)) => (negative_amount(amount), withdraw_ext_hash(to.as_str())),
        None => (Fr::zero(), Fr::zero()),
    };
    let circuit = JoinSplitCircuit::new(root, asset_id, inputs, outputs, public_amount, ext_hash);
    let public = circuit.public.clone();
    let proof = keys::prove(&keys.joinsplit, circuit, rand::random());
    let mut args = json!({
        "proof": b64(&near_encoding::proof_bytes(&proof)),
        "root": fr_to_hex(&public.root),
        "asset_id": asset_id,
        "nullifiers": [fr_to_hex(&public.nullifiers[0]), fr_to_hex(&public.nullifiers[1])],
        "commitments": [fr_to_hex(&public.commitments[0]), fr_to_hex(&public.commitments[1])],
        "ciphertexts": [b64(&ciphertexts[0]), b64(&ciphertexts[1])],
    });
    if let Some((amount, to)) = withdraw {
        args["withdraw_amount"] = json!(amount.to_string());
        args["withdraw_to"] = json!(to);
    }
    let r = ensure_ok(chain.call("joinsplit", args).await?)?;
    for i in idxs {
        wallet.notes[i].spent = true;
    }
    Ok(r)
}

/// Private transfer of `amount` of `asset_id` to a shielded address; change returns to this wallet.
pub async fn transfer(
    wallet: &mut WalletFile,
    chain: &Chain,
    keys: &ProvingKeys,
    asset_id: u32,
    amount: u128,
    to: Address,
) -> Result<ExecutionFinalResult> {
    let idxs = wallet.select_inputs(asset_id, amount)?;
    let (_, total) = inputs_for(wallet, &idxs, asset_id)?;
    let me = wallet.address();
    let out = Note {
        asset_id,
        amount,
        pk: to.pk,
        rseed: random_fr(),
    };
    let change = Note {
        asset_id,
        amount: total - amount,
        pk: me.pk,
        rseed: random_fr(),
    };
    let cts = [
        seal(&to, note_plaintext(&out)),
        seal(&me, note_plaintext(&change)),
    ];
    submit_joinsplit(
        wallet,
        chain,
        keys,
        asset_id,
        idxs,
        [out, change],
        cts,
        None,
    )
    .await
}

/// Withdraws `amount` of `asset_id` to a NEAR account; change stays shielded.
pub async fn withdraw(
    wallet: &mut WalletFile,
    chain: &Chain,
    keys: &ProvingKeys,
    asset_id: u32,
    amount: u128,
    to: &AccountId,
) -> Result<ExecutionFinalResult> {
    let idxs = wallet.select_inputs(asset_id, amount)?;
    let (_, total) = inputs_for(wallet, &idxs, asset_id)?;
    let me = wallet.address();
    let change = Note {
        asset_id,
        amount: total - amount,
        pk: me.pk,
        rseed: random_fr(),
    };
    let zero = Note::dummy(asset_id, me.pk, random_fr());
    let cts = [seal(&me, note_plaintext(&change)), vec![]];
    submit_joinsplit(
        wallet,
        chain,
        keys,
        asset_id,
        idxs,
        [change, zero],
        cts,
        Some((amount, to)),
    )
    .await
}

/// Sells `amount` of `sell_asset` on `pair_id` in the current epoch.
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
    let direction = if sell_asset == pair.asset_1 {
        DIR_1_TO_2
    } else if sell_asset == pair.asset_2 {
        DIR_2_TO_1
    } else {
        return Err(anyhow!("asset {sell_asset} is not part of pair {pair_id}"));
    };
    let epoch = chain.current_epoch().await?.epoch;
    let idxs = wallet.select_inputs(sell_asset, amount)?;
    let (inputs, total) = inputs_for(wallet, &idxs, sell_asset)?;
    let me = wallet.address();
    let change = Note {
        asset_id: sell_asset,
        amount: total - amount,
        pk: me.pk,
        rseed: random_fr(),
    };
    let record = SwapRecord {
        epoch,
        pair_id,
        direction,
        delta: amount,
        claim_pk: me.pk,
        rseed: random_fr(),
    };
    let root = tree::root_of(&wallet.leaves_fr());
    let circuit = SwapCircuit::new(root, sell_asset, inputs, change, record);
    let public = circuit.public.clone();
    let proof = keys::prove(&keys.swap, circuit, rand::random());
    let swap_ct = seal(
        &me,
        Plaintext::Swap(SwapPlaintext {
            epoch,
            pair_id,
            direction,
            delta: amount,
            rseed: record.rseed,
        }),
    );
    let args = json!({
        "proof": b64(&near_encoding::proof_bytes(&proof)),
        "root": fr_to_hex(&public.root),
        "asset_id": sell_asset,
        "nullifiers": [fr_to_hex(&public.nullifiers[0]), fr_to_hex(&public.nullifiers[1])],
        "change_commitment": fr_to_hex(&public.change_commitment),
        "change_ciphertext": b64(&seal(&me, note_plaintext(&change))),
        "swap_commitment": fr_to_hex(&public.swap_commitment),
        "swap_ciphertext": b64(&swap_ct),
        "epoch": epoch,
        "pair_id": pair_id,
        "direction": direction,
        "delta": amount.to_string(),
    });
    let r = ensure_ok(chain.call("swap", args).await?)?;
    for i in idxs {
        wallet.notes[i].spent = true;
    }
    Ok((epoch, r))
}

/// Executes a finished epoch for a pair (default: the most recent finished one).
pub async fn end_epoch(
    chain: &Chain,
    pair_id: u32,
    epoch: Option<u64>,
) -> Result<(u64, ExecutionFinalResult)> {
    let epoch = match epoch {
        Some(e) => e,
        None => chain
            .current_epoch()
            .await?
            .epoch
            .checked_sub(1)
            .ok_or_else(|| anyhow!("no finished epoch yet"))?,
    };
    let r = ensure_ok(
        chain
            .call("end_epoch", json!({"epoch": epoch, "pair_id": pair_id}))
            .await?,
    )?;
    Ok((epoch, r))
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
}

/// Claims every claimable swap, executing finished batches on the way.
pub async fn claim_all(
    wallet: &mut WalletFile,
    chain: &Chain,
    keys: &ProvingKeys,
) -> Result<Vec<ClaimOutcome>> {
    let mut outcomes = Vec::new();
    let now = chain.current_epoch().await?;
    let sk = wallet.spend_key();
    for i in 0..wallet.swaps.len() {
        if wallet.swaps[i].claimed {
            continue;
        }
        let entry = wallet.swaps[i].clone();
        let record = wallet.swap(&entry);
        if entry.epoch >= now.epoch {
            let blocks_left = (entry.epoch - now.epoch) * 20 + now.blocks_left;
            outcomes.push(ClaimOutcome::Waiting {
                index: i,
                epoch: entry.epoch,
                blocks_left,
            });
            continue;
        }
        let mut batch = chain.get_batch(entry.epoch, entry.pair_id).await?;
        if !batch.as_ref().map(|b| b.executed).unwrap_or(false) {
            let _ = ensure_ok(
                chain
                    .call(
                        "end_epoch",
                        json!({"epoch": entry.epoch, "pair_id": entry.pair_id}),
                    )
                    .await?,
            )?;
            batch = chain.get_batch(entry.epoch, entry.pair_id).await?;
        }
        let b = batch.ok_or_else(|| anyhow!("batch missing after execution"))?;
        let pair = chain
            .get_pair(entry.pair_id)
            .await?
            .ok_or_else(|| anyhow!("unknown pair"))?;
        let bsod = Bsod {
            epoch: entry.epoch,
            pair_id: entry.pair_id,
            asset_1: pair.asset_1,
            asset_2: pair.asset_2,
            delta_1: b.delta_1.parse()?,
            delta_2: b.delta_2.parse()?,
            lambda_1: b.lambda_1.parse()?,
            lambda_2: b.lambda_2.parse()?,
            unfilled_1: b.unfilled_1.parse()?,
            unfilled_2: b.unfilled_2.parse()?,
        };
        if Some(fr_to_hex(&bsod.hash())) != b.bsod_hash {
            return Err(anyhow!(
                "batch hash mismatch for epoch {} pair {}",
                entry.epoch,
                entry.pair_id
            ));
        }
        let leaves = wallet.leaves_fr();
        if entry.position as usize >= leaves.len() {
            return Err(anyhow!("wallet is behind the chain, run `sync` first"));
        }
        let circuit = SwapClaimCircuit::new(
            tree::root_of(&leaves),
            record,
            sk,
            entry.position,
            tree::merkle_path(&leaves, entry.position),
            bsod,
            [random_fr(), random_fr()],
        );
        let outputs = circuit.outputs;
        let public = circuit.public.clone();
        let proof = keys::prove(&keys.swapclaim, circuit, rand::random());
        let me = wallet.address();
        let args = json!({
            "proof": b64(&near_encoding::proof_bytes(&proof)),
            "root": fr_to_hex(&public.root),
            "nullifier": fr_to_hex(&public.nullifier),
            "commitments": [fr_to_hex(&public.commitments[0]), fr_to_hex(&public.commitments[1])],
            "ciphertexts": [b64(&seal(&me, note_plaintext(&outputs[0]))), b64(&seal(&me, note_plaintext(&outputs[1])))],
            "epoch": entry.epoch,
            "pair_id": entry.pair_id,
        });
        let r = ensure_ok(chain.call("swap_claim", args).await?)?;
        wallet.swaps[i].claimed = true;
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
