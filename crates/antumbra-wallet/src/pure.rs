//! Network-free wallet operations: scanning, note selection, proving and
//! building the exact contract call arguments. Shared by the native CLI and
//! the browser build; randomness is always supplied by the caller.

use crate::{
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
use ark_std::rand::{rngs::StdRng, RngCore, SeedableRng};
use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

pub type Rng = StdRng;

pub fn rng_from_seed(seed: [u8; 32]) -> Rng {
    StdRng::from_seed(seed)
}

fn seed32(rng: &mut Rng) -> [u8; 32] {
    let mut s = [0u8; 32];
    rng.fill_bytes(&mut s);
    s
}

fn random_fr(rng: &mut Rng) -> Fr {
    Fr::rand(rng)
}

pub fn b64(bytes: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

fn seal(to: &Address, pt: Plaintext, rng: &mut Rng) -> Vec<u8> {
    crypto::encrypt_with_seed(&to.public_key(), &pt.to_bytes(), seed32(rng))
}

fn note_plaintext(n: &Note) -> Plaintext {
    Plaintext::Note(NotePlaintext {
        asset_id: n.asset_id,
        amount: n.amount,
        rseed: n.rseed,
    })
}

/// One leaf of the on-chain log, as the contract's `get_leaves` returns it.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LeafData {
    pub position: u64,
    pub cm: String,
    /// base64
    pub ciphertext: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct ScanReport {
    pub new_leaves: u64,
    pub new_notes: usize,
    pub new_swaps: usize,
}

/// Appends new leaves to the wallet's cache and trial-decrypts them.
/// Leaves must be contiguous and start at the cache length.
pub fn scan_leaves(wallet: &mut WalletFile, leaves: &[LeafData]) -> Result<ScanReport> {
    let mut report = ScanReport::default();
    let me = wallet.address();
    let enc = wallet.enc_key();
    for leaf in leaves {
        if leaf.position < wallet.leaves.len() as u64 {
            continue; // already known
        }
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
        let Ok(blob) = base64::engine::general_purpose::STANDARD.decode(&leaf.ciphertext) else {
            continue;
        };
        let Some(pt) = crypto::decrypt(&enc.secret, &blob).and_then(|b| Plaintext::from_bytes(&b))
        else {
            continue;
        };
        match pt {
            Plaintext::Note(n) => {
                let note = Note {
                    asset_id: n.asset_id,
                    amount: n.amount,
                    pk: me.pk,
                    rseed: n.rseed,
                };
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
                    claim_pk: me.pk,
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
    Ok(report)
}

/// `(index, nullifier hex)` for every unspent note.
pub fn note_nullifiers(wallet: &WalletFile) -> Vec<(usize, String)> {
    let nk = wallet.spend_key().nk();
    wallet
        .notes
        .iter()
        .enumerate()
        .filter(|(_, n)| !n.spent)
        .map(|(i, e)| (i, fr_to_hex(&wallet.note(e).nullifier(nk, e.position))))
        .collect()
}

/// `(index, nullifier hex)` for every unclaimed swap.
pub fn swap_nullifiers(wallet: &WalletFile) -> Vec<(usize, String)> {
    let nk = wallet.spend_key().nk();
    wallet
        .swaps
        .iter()
        .enumerate()
        .filter(|(_, s)| !s.claimed)
        .map(|(i, e)| (i, fr_to_hex(&wallet.swap(e).nullifier(nk, e.position))))
        .collect()
}

pub fn mark_spent_notes(wallet: &mut WalletFile, idxs: &[usize]) {
    for &i in idxs {
        if let Some(n) = wallet.notes.get_mut(i) {
            n.spent = true;
        }
    }
}

pub fn mark_claimed_swaps(wallet: &mut WalletFile, idxs: &[usize]) {
    for &i in idxs {
        if let Some(s) = wallet.swaps.get_mut(i) {
            s.claimed = true;
        }
    }
}

/// A contract call ready to be signed, plus the local bookkeeping to apply once it succeeds.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Prepared {
    pub method: String,
    pub args: Value,
    #[serde(default)]
    pub spent_notes: Vec<usize>,
    #[serde(default)]
    pub claimed_swap: Option<usize>,
    /// Human-readable summary of what the chain will see.
    pub public_view: Value,
}

/// `ft_transfer_call` message for a deposit into a fresh note owned by this wallet.
pub fn prepare_deposit(
    wallet: &WalletFile,
    asset_id: u32,
    amount: u128,
    rng: &mut Rng,
) -> (Value, Note) {
    let me = wallet.address();
    let note = Note {
        asset_id,
        amount,
        pk: me.pk,
        rseed: random_fr(rng),
    };
    let msg = json!({"cm": fr_to_hex(&note.commitment()), "ciphertext": b64(&seal(&me, note_plaintext(&note), rng))});
    (msg, note)
}

/// Input witnesses for the selected notes; pads with a dummy.
fn inputs_for(
    wallet: &WalletFile,
    idxs: &[usize],
    asset_id: u32,
    rng: &mut Rng,
) -> Result<([InputWitness; 2], u128)> {
    let leaves = wallet.leaves_fr();
    let sk = wallet.spend_key();
    let mut total = 0u128;
    let mut witnesses = Vec::new();
    for &i in idxs {
        let e = &wallet.notes[i];
        let note = wallet.note(e);
        if e.position as usize >= leaves.len() {
            return Err(anyhow!("wallet is behind the chain, sync first"));
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
        witnesses.push(InputWitness::dummy(asset_id, random_fr(rng)));
    }
    let arr: [InputWitness; 2] = witnesses
        .try_into()
        .map_err(|_| anyhow!("two inputs expected"))?;
    Ok((arr, total))
}

#[allow(clippy::too_many_arguments)]
fn prepare_joinsplit(
    wallet: &WalletFile,
    keys: &ProvingKeys,
    asset_id: u32,
    idxs: Vec<usize>,
    outputs: [Note; 2],
    ciphertexts: [Vec<u8>; 2],
    withdraw: Option<(u128, &str)>,
    rng: &mut Rng,
) -> Result<Prepared> {
    let (inputs, _) = inputs_for(wallet, &idxs, asset_id, rng)?;
    let root = tree::root_of(&wallet.leaves_fr());
    let (public_amount, ext_hash) = match withdraw {
        Some((amount, to)) => (negative_amount(amount), withdraw_ext_hash(to)),
        None => (Fr::zero(), Fr::zero()),
    };
    let circuit = JoinSplitCircuit::new(root, asset_id, inputs, outputs, public_amount, ext_hash);
    let public = circuit.public.clone();
    let proof = keys::prove(&keys.joinsplit, circuit, seed32(rng));
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
    let public_view = json!({
        "nullifiers": args["nullifiers"], "commitments": args["commitments"],
        "withdraw": withdraw.map(|(a, t)| json!({"amount": a.to_string(), "to": t})),
    });
    Ok(Prepared {
        method: "joinsplit".into(),
        args,
        spent_notes: idxs,
        claimed_swap: None,
        public_view,
    })
}

/// Private transfer of `amount` of `asset_id` to a shielded address; change returns to this wallet.
pub fn prepare_transfer(
    wallet: &WalletFile,
    keys: &ProvingKeys,
    asset_id: u32,
    amount: u128,
    to: Address,
    rng: &mut Rng,
) -> Result<Prepared> {
    let idxs = wallet.select_inputs(asset_id, amount)?;
    let (_, total) = inputs_for(wallet, &idxs, asset_id, rng)?;
    let me = wallet.address();
    let out = Note {
        asset_id,
        amount,
        pk: to.pk,
        rseed: random_fr(rng),
    };
    let change = Note {
        asset_id,
        amount: total - amount,
        pk: me.pk,
        rseed: random_fr(rng),
    };
    let cts = [
        seal(&to, note_plaintext(&out), rng),
        seal(&me, note_plaintext(&change), rng),
    ];
    prepare_joinsplit(wallet, keys, asset_id, idxs, [out, change], cts, None, rng)
}

/// Withdraws `amount` of `asset_id` to a NEAR account; change stays shielded.
pub fn prepare_withdraw(
    wallet: &WalletFile,
    keys: &ProvingKeys,
    asset_id: u32,
    amount: u128,
    to_account: &str,
    rng: &mut Rng,
) -> Result<Prepared> {
    let idxs = wallet.select_inputs(asset_id, amount)?;
    let (_, total) = inputs_for(wallet, &idxs, asset_id, rng)?;
    let me = wallet.address();
    let change = Note {
        asset_id,
        amount: total - amount,
        pk: me.pk,
        rseed: random_fr(rng),
    };
    let zero = Note::dummy(asset_id, me.pk, random_fr(rng));
    let cts = [seal(&me, note_plaintext(&change), rng), vec![]];
    prepare_joinsplit(
        wallet,
        keys,
        asset_id,
        idxs,
        [change, zero],
        cts,
        Some((amount, to_account)),
        rng,
    )
}

/// Sells `amount` of `sell_asset` on a pair `(asset_1, asset_2)` in `epoch`.
#[allow(clippy::too_many_arguments)]
pub fn prepare_swap(
    wallet: &WalletFile,
    keys: &ProvingKeys,
    pair_id: u32,
    pair_assets: (u32, u32),
    sell_asset: u32,
    amount: u128,
    epoch: u64,
    rng: &mut Rng,
) -> Result<Prepared> {
    let direction = if sell_asset == pair_assets.0 {
        DIR_1_TO_2
    } else if sell_asset == pair_assets.1 {
        DIR_2_TO_1
    } else {
        return Err(anyhow!("asset {sell_asset} is not part of pair {pair_id}"));
    };
    let idxs = wallet.select_inputs(sell_asset, amount)?;
    let (inputs, total) = inputs_for(wallet, &idxs, sell_asset, rng)?;
    let me = wallet.address();
    let change = Note {
        asset_id: sell_asset,
        amount: total - amount,
        pk: me.pk,
        rseed: random_fr(rng),
    };
    let record = SwapRecord {
        epoch,
        pair_id,
        direction,
        delta: amount,
        claim_pk: me.pk,
        rseed: random_fr(rng),
    };
    let root = tree::root_of(&wallet.leaves_fr());
    let circuit = SwapCircuit::new(root, sell_asset, inputs, change, record);
    let public = circuit.public.clone();
    let proof = keys::prove(&keys.swap, circuit, seed32(rng));
    let swap_ct = seal(
        &me,
        Plaintext::Swap(SwapPlaintext {
            epoch,
            pair_id,
            direction,
            delta: amount,
            rseed: record.rseed,
        }),
        rng,
    );
    let args = json!({
        "proof": b64(&near_encoding::proof_bytes(&proof)),
        "root": fr_to_hex(&public.root),
        "asset_id": sell_asset,
        "nullifiers": [fr_to_hex(&public.nullifiers[0]), fr_to_hex(&public.nullifiers[1])],
        "change_commitment": fr_to_hex(&public.change_commitment),
        "change_ciphertext": b64(&seal(&me, note_plaintext(&change), rng)),
        "swap_commitment": fr_to_hex(&public.swap_commitment),
        "swap_ciphertext": b64(&swap_ct),
        "epoch": epoch,
        "pair_id": pair_id,
        "direction": direction,
        "delta": amount.to_string(),
    });
    let public_view = json!({"epoch": epoch, "pair_id": pair_id, "direction": direction, "delta": amount.to_string(), "nullifiers": args["nullifiers"]});
    Ok(Prepared {
        method: "swap".into(),
        args,
        spent_notes: idxs,
        claimed_swap: None,
        public_view,
    })
}

/// Claims swap `index` against an executed batch result. Returns the call and the two output notes.
pub fn prepare_claim(
    wallet: &WalletFile,
    keys: &ProvingKeys,
    index: usize,
    bsod: &Bsod,
    rng: &mut Rng,
) -> Result<(Prepared, [Note; 2])> {
    let entry = wallet
        .swaps
        .get(index)
        .ok_or_else(|| anyhow!("no swap #{index}"))?;
    if entry.claimed {
        return Err(anyhow!("swap #{index} is already claimed"));
    }
    if (bsod.epoch, bsod.pair_id) != (entry.epoch, entry.pair_id) {
        return Err(anyhow!("batch does not match swap #{index}"));
    }
    let record = wallet.swap(entry);
    let leaves = wallet.leaves_fr();
    if entry.position as usize >= leaves.len() {
        return Err(anyhow!("wallet is behind the chain, sync first"));
    }
    let circuit = SwapClaimCircuit::new(
        tree::root_of(&leaves),
        record,
        wallet.spend_key(),
        entry.position,
        tree::merkle_path(&leaves, entry.position),
        *bsod,
        [random_fr(rng), random_fr(rng)],
    );
    let outputs = circuit.outputs;
    let public = circuit.public.clone();
    let proof = keys::prove(&keys.swapclaim, circuit, seed32(rng));
    let me = wallet.address();
    let args = json!({
        "proof": b64(&near_encoding::proof_bytes(&proof)),
        "root": fr_to_hex(&public.root),
        "nullifier": fr_to_hex(&public.nullifier),
        "commitments": [fr_to_hex(&public.commitments[0]), fr_to_hex(&public.commitments[1])],
        "ciphertexts": [b64(&seal(&me, note_plaintext(&outputs[0]), rng)), b64(&seal(&me, note_plaintext(&outputs[1]), rng))],
        "epoch": entry.epoch,
        "pair_id": entry.pair_id,
    });
    let public_view = json!({"epoch": entry.epoch, "pair_id": entry.pair_id, "nullifier": args["nullifier"], "commitments": args["commitments"]});
    Ok((
        Prepared {
            method: "swap_claim".into(),
            args,
            spent_notes: vec![],
            claimed_swap: Some(index),
            public_view,
        },
        outputs,
    ))
}

/// Builds a `Bsod` from the contract's `get_batch` view plus the pair's assets.
pub fn bsod_from_view(epoch: u64, pair_id: u32, assets: (u32, u32), v: &Value) -> Result<Bsod> {
    let f = |k: &str| -> Result<u128> {
        Ok(v[k]
            .as_str()
            .ok_or_else(|| anyhow!("batch field {k}"))?
            .parse()?)
    };
    let bsod = Bsod {
        epoch,
        pair_id,
        asset_1: assets.0,
        asset_2: assets.1,
        delta_1: f("delta_1")?,
        delta_2: f("delta_2")?,
        lambda_1: f("lambda_1")?,
        lambda_2: f("lambda_2")?,
        unfilled_1: f("unfilled_1")?,
        unfilled_2: f("unfilled_2")?,
    };
    if let Some(h) = v["bsod_hash"].as_str() {
        if h != fr_to_hex(&bsod.hash()) {
            return Err(anyhow!(
                "batch hash mismatch for epoch {epoch} pair {pair_id}"
            ));
        }
    }
    Ok(bsod)
}
