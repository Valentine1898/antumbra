//! End-to-end tests on a local NEAR sandbox.
//!
//! 1. `deposit_transfer_scan_withdraw`: deposit, private transfer, wallet
//!    scanning, withdrawal, and two rejected attacks.
//! 2. `swap_cycle`: liquidity, a shielded swap, epoch execution at one
//!    clearing price, a private claim, and withdrawal of the bought asset.
//!
//! Run: scripts/build-contracts.sh && CARGO_TARGET_DIR=target/tests cargo test -p antumbra-e2e -- --nocapture

use antumbra_circuits::{
    joinsplit::{InputWitness, JoinSplitCircuit},
    keys, near_encoding,
    swap::SwapCircuit,
    swapclaim::SwapClaimCircuit,
    Bls12_381, ProvingKey,
};
use antumbra_core::{
    amm,
    crypto::{self, EncryptionKey, NotePlaintext, Plaintext, SwapPlaintext},
    encoding::{fr_from_hex, fr_to_hex, negative_amount},
    note::{withdraw_ext_hash, Bsod, Note, SpendKey, SwapRecord, DIR_1_TO_2},
    tree, Fr,
};
use anyhow::{anyhow, Result};
use ark_ff::{UniformRand, Zero};
use ark_std::rand::{rngs::StdRng, SeedableRng};
use base64::Engine;
use near_workspaces::{
    network::Sandbox, result::ExecutionFinalResult, types::NearToken, Account, Contract, Worker,
};
use serde_json::{json, Value};

const TGAS: f64 = 1e12;
const EPOCH_BLOCKS: u64 = 20;

fn wasm(name: &str) -> Result<Vec<u8>> {
    let p = format!(
        "{}/../../target/wasm32-unknown-unknown/release/{name}.wasm",
        env!("CARGO_MANIFEST_DIR")
    );
    std::fs::read(&p).map_err(|e| anyhow!("{p}: {e} (run scripts/build-contracts.sh first)"))
}

fn b64(bytes: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

fn random_fr() -> Fr {
    Fr::rand(&mut StdRng::from_seed(rand::random()))
}

fn gas(label: &str, r: &ExecutionFinalResult) {
    let receipt = r
        .receipt_outcomes()
        .first()
        .map(|o| o.gas_burnt.as_gas())
        .unwrap_or(0) as f64
        / TGAS;
    let total = r.total_gas_burnt.as_gas() as f64 / TGAS;
    let logs: Vec<&str> = r
        .logs()
        .into_iter()
        .filter(|l| !l.starts_with("EVENT_JSON"))
        .collect();
    println!("{label:<34} receipt={receipt:>7.2} Tgas  total={total:>7.2} Tgas  {logs:?}");
    if r.is_failure() {
        println!("    FAILURES: {:?}", r.failures());
    }
}

fn failure_text(r: &ExecutionFinalResult) -> String {
    format!("{:?}", r.failures())
}

// ----- a minimal wallet -----

struct Wallet {
    sk: SpendKey,
    enc: EncryptionKey,
    notes: Vec<(Note, u64)>,
    swaps: Vec<(SwapRecord, u64)>,
}

impl Wallet {
    fn random() -> Self {
        Wallet {
            sk: SpendKey(random_fr()),
            enc: EncryptionKey::random(),
            notes: vec![],
            swaps: vec![],
        }
    }
    fn pk(&self) -> Fr {
        self.sk.pk()
    }
    fn make_note(&self, asset_id: u32, amount: u128) -> Note {
        Note {
            asset_id,
            amount,
            pk: self.pk(),
            rseed: random_fr(),
        }
    }
    fn seal_note(&self, note: &Note) -> Vec<u8> {
        let pt = Plaintext::Note(NotePlaintext {
            asset_id: note.asset_id,
            amount: note.amount,
            rseed: note.rseed,
        });
        crypto::encrypt(&self.enc.public, &pt.to_bytes())
    }
    fn seal_swap(&self, s: &SwapRecord) -> Vec<u8> {
        let pt = Plaintext::Swap(SwapPlaintext {
            epoch: s.epoch,
            pair_id: s.pair_id,
            direction: s.direction,
            delta: s.delta,
            rseed: s.rseed,
        });
        crypto::encrypt(&self.enc.public, &pt.to_bytes())
    }
    /// Trial-decrypts every leaf and keeps what is locked to this wallet.
    fn scan(&mut self, leaves: &[(u64, Fr, Vec<u8>)]) {
        for (position, cm, blob) in leaves {
            if self.notes.iter().any(|(_, p)| p == position)
                || self.swaps.iter().any(|(_, p)| p == position)
            {
                continue;
            }
            let Some(pt) =
                crypto::decrypt(&self.enc.secret, blob).and_then(|b| Plaintext::from_bytes(&b))
            else {
                continue;
            };
            match pt {
                Plaintext::Note(n) => {
                    let note = Note {
                        asset_id: n.asset_id,
                        amount: n.amount,
                        pk: self.pk(),
                        rseed: n.rseed,
                    };
                    if note.commitment() == *cm {
                        self.notes.push((note, *position));
                    }
                }
                Plaintext::Swap(s) => {
                    let swap = SwapRecord {
                        epoch: s.epoch,
                        pair_id: s.pair_id,
                        direction: s.direction,
                        delta: s.delta,
                        claim_pk: self.pk(),
                        rseed: s.rseed,
                    };
                    if swap.commitment() == *cm {
                        self.swaps.push((swap, *position));
                    }
                }
            }
        }
    }
    fn note_of(&self, asset_id: u32, amount: u128) -> (Note, u64) {
        *self
            .notes
            .iter()
            .find(|(n, _)| n.asset_id == asset_id && n.amount == amount)
            .expect("note present")
    }
}

// ----- chain helpers -----

struct Chain {
    worker: Worker<Sandbox>,
    pool: Contract,
    tokens: Vec<Contract>,
    alice: Account,
    bob: Account,
}

async fn deploy(n_tokens: usize) -> Result<Chain> {
    let worker = near_workspaces::sandbox().await?;
    let pool = worker.dev_deploy(&wasm("antumbra_pool")?).await?;
    pool.call("new")
        .args_json(json!({"owner": pool.id()}))
        .max_gas()
        .transact()
        .await?
        .into_result()?;
    let alice = worker.dev_create_account().await?;
    let bob = worker.dev_create_account().await?;
    let mut tokens = vec![];
    for i in 0..n_tokens {
        let ft = worker.dev_deploy(&wasm("test_ft")?).await?;
        ft.call("new").max_gas().transact().await?.into_result()?;
        let id: u32 = pool
            .call("register_asset")
            .args_json(json!({"token": ft.id()}))
            .max_gas()
            .transact()
            .await?
            .into_result()?
            .json()?;
        assert_eq!(id as usize, i + 1);
        for who in [pool.id(), bob.id()] {
            alice
                .call(ft.id(), "storage_deposit")
                .args_json(json!({"account_id": who}))
                .deposit(NearToken::from_millinear(10))
                .max_gas()
                .transact()
                .await?
                .into_result()?;
        }
        tokens.push(ft);
    }
    Ok(Chain {
        worker,
        pool,
        tokens,
        alice,
        bob,
    })
}

async fn mint(ft: &Contract, to: &Account, amount: u128) -> Result<()> {
    ft.call("mint")
        .args_json(json!({"account_id": to.id(), "amount": amount.to_string()}))
        .max_gas()
        .transact()
        .await?
        .into_result()?;
    Ok(())
}

async fn balance(ft: &Contract, who: &Account) -> Result<u128> {
    Ok(ft
        .view("ft_balance_of")
        .args_json(json!({"account_id": who.id()}))
        .await?
        .json::<String>()?
        .parse()?)
}

async fn transfer_call(
    from: &Account,
    ft: &Contract,
    pool: &Contract,
    amount: u128,
    msg: Value,
) -> Result<ExecutionFinalResult> {
    Ok(from
        .call(ft.id(), "ft_transfer_call")
        .args_json(
            json!({"receiver_id": pool.id(), "amount": amount.to_string(), "msg": msg.to_string()}),
        )
        .deposit(NearToken::from_yoctonear(1))
        .max_gas()
        .transact()
        .await?)
}

async fn fetch_leaves(pool: &Contract) -> Result<Vec<(u64, Fr, Vec<u8>)>> {
    let count: u64 = pool.view("leaf_count").await?.json()?;
    let views: Vec<Value> = pool
        .view("get_leaves")
        .args_json(json!({"from": 0, "limit": count}))
        .await?
        .json()?;
    views
        .into_iter()
        .map(|v| {
            let position = v["position"].as_u64().unwrap();
            let cm = fr_from_hex(v["cm"].as_str().unwrap()).ok_or_else(|| anyhow!("bad cm"))?;
            let ct = base64::engine::general_purpose::STANDARD
                .decode(v["ciphertext"].as_str().unwrap())?;
            Ok((position, cm, ct))
        })
        .collect()
}

/// Returns the current epoch, first skipping ahead if it is about to end.
async fn epoch_with_room(chain: &Chain, min_blocks_left: u64) -> Result<u64> {
    let v: Value = chain.pool.view("current_epoch").await?.json()?;
    let (epoch, left) = (
        v["epoch"].as_u64().unwrap(),
        v["blocks_left"].as_u64().unwrap(),
    );
    if left < min_blocks_left {
        chain.worker.fast_forward(left + 1).await?;
        let v: Value = chain.pool.view("current_epoch").await?.json()?;
        return Ok(v["epoch"].as_u64().unwrap());
    }
    Ok(epoch)
}

#[allow(clippy::too_many_arguments)]
async fn joinsplit_call(
    caller: &Account,
    pool: &Contract,
    pk: &ProvingKey<Bls12_381>,
    leaves: &[Fr],
    asset_id: u32,
    inputs: [InputWitness; 2],
    outputs: [Note; 2],
    ciphertexts: [Vec<u8>; 2],
    withdraw: Option<(u128, &Account)>,
) -> Result<ExecutionFinalResult> {
    let root = tree::root_of(leaves);
    let (public_amount, ext_hash) = match withdraw {
        Some((amount, to)) => (negative_amount(amount), withdraw_ext_hash(to.id().as_str())),
        None => (Fr::zero(), Fr::zero()),
    };
    let circuit = JoinSplitCircuit::new(root, asset_id, inputs, outputs, public_amount, ext_hash);
    let public = circuit.public.clone();
    let proof = keys::prove(pk, circuit, rand::random());
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
        args["withdraw_to"] = json!(to.id());
    }
    Ok(caller
        .call(pool.id(), "joinsplit")
        .args_json(args)
        .max_gas()
        .transact()
        .await?)
}

#[tokio::test]
async fn deposit_transfer_scan_withdraw() -> Result<()> {
    let chain = deploy(1).await?;
    let (pool, ft, alice, bob) = (&chain.pool, &chain.tokens[0], &chain.alice, &chain.bob);
    mint(ft, alice, 1_000_000).await?;
    let (pk, _) = keys::joinsplit_setup();
    let mut wa = Wallet::random();
    let mut wb = Wallet::random();
    let mut leaves: Vec<Fr> = vec![];

    // 1. Alice deposits 600_000 into a note (public amount, hidden owner).
    let note_a = wa.make_note(1, 600_000);
    let r = transfer_call(
        alice,
        ft,
        pool,
        600_000,
        json!({"cm": fr_to_hex(&note_a.commitment()), "ciphertext": b64(&wa.seal_note(&note_a))}),
    )
    .await?;
    gas("deposit (ft_transfer_call)", &r);
    r.into_result()?;
    leaves.push(note_a.commitment());
    assert_eq!(pool.view("leaf_count").await?.json::<u64>()?, 1);
    assert_eq!(balance(ft, &pool.as_account().clone()).await?, 600_000);
    assert_eq!(
        pool.view("get_root").await?.json::<String>()?,
        fr_to_hex(&tree::root_of(&leaves))
    );
    wa.scan(&fetch_leaves(pool).await?);
    assert_eq!(wa.notes.len(), 1);

    // 2. Alice privately sends 250_000 to Bob, keeps 350_000.
    let (a_note, a_pos) = wa.note_of(1, 600_000);
    let to_bob = wb.make_note(1, 250_000);
    let change = wa.make_note(1, 350_000);
    let inputs = [
        InputWitness {
            note: a_note,
            sk: wa.sk,
            position: a_pos,
            siblings: tree::merkle_path(&leaves, a_pos),
        },
        InputWitness::dummy(1, random_fr()),
    ];
    let nf_a = inputs[0].nullifier();
    let r = joinsplit_call(
        alice,
        pool,
        &pk,
        &leaves,
        1,
        inputs,
        [to_bob, change],
        [wb.seal_note(&to_bob), wa.seal_note(&change)],
        None,
    )
    .await?;
    gas("joinsplit transfer", &r);
    r.into_result()?;
    leaves.extend([to_bob.commitment(), change.commitment()]);
    assert!(pool
        .view("is_spent")
        .args_json(json!({"nullifier": fr_to_hex(&nf_a)}))
        .await?
        .json::<bool>()?);

    // 3. Bob scans the ciphertext log and finds exactly his note.
    let chain_leaves = fetch_leaves(pool).await?;
    wb.scan(&chain_leaves);
    assert_eq!(wb.notes.len(), 1);
    assert_eq!(wb.notes[0].0.amount, 250_000);
    assert_eq!(wb.notes[0].1, 1);
    wa.scan(&chain_leaves);
    assert_eq!(wa.notes.len(), 2);

    // 4. Bob withdraws 100_000 to his NEAR account; change 150_000 stays shielded.
    let (b_note, b_pos) = wb.note_of(1, 250_000);
    let b_change = wb.make_note(1, 150_000);
    let zero_out = Note::dummy(1, wb.pk(), random_fr());
    let inputs = [
        InputWitness {
            note: b_note,
            sk: wb.sk,
            position: b_pos,
            siblings: tree::merkle_path(&leaves, b_pos),
        },
        InputWitness::dummy(1, random_fr()),
    ];
    let r = joinsplit_call(
        bob,
        pool,
        &pk,
        &leaves,
        1,
        inputs.clone(),
        [b_change, zero_out],
        [wb.seal_note(&b_change), wb.seal_note(&zero_out)],
        Some((100_000, bob)),
    )
    .await?;
    gas("joinsplit withdraw", &r);
    r.into_result()?;
    leaves.extend([b_change.commitment(), zero_out.commitment()]);
    assert_eq!(balance(ft, bob).await?, 100_000);
    assert_eq!(balance(ft, &pool.as_account().clone()).await?, 500_000);

    // 5. Replaying the spent note fails; a proof bound to Bob cannot pay Alice.
    let r = joinsplit_call(
        bob,
        pool,
        &pk,
        &leaves[..3],
        1,
        inputs,
        [b_change, zero_out],
        [vec![], vec![]],
        Some((100_000, bob)),
    )
    .await?;
    assert!(
        r.is_failure() && failure_text(&r).contains("already spent"),
        "{}",
        failure_text(&r)
    );
    let (c_note, c_pos) = (b_change, 3u64);
    let inputs = [
        InputWitness {
            note: c_note,
            sk: wb.sk,
            position: c_pos,
            siblings: tree::merkle_path(&leaves, c_pos),
        },
        InputWitness::dummy(1, random_fr()),
    ];
    let circuit = JoinSplitCircuit::new(
        tree::root_of(&leaves),
        1,
        inputs,
        [
            wb.make_note(1, 50_000),
            Note::dummy(1, wb.pk(), random_fr()),
        ],
        negative_amount(100_000),
        withdraw_ext_hash(bob.id().as_str()),
    );
    let public = circuit.public.clone();
    let proof = keys::prove(&pk, circuit, rand::random());
    let r = bob
        .call(pool.id(), "joinsplit")
        .args_json(json!({
            "proof": b64(&near_encoding::proof_bytes(&proof)),
            "root": fr_to_hex(&public.root),
            "asset_id": 1,
            "nullifiers": [fr_to_hex(&public.nullifiers[0]), fr_to_hex(&public.nullifiers[1])],
            "commitments": [fr_to_hex(&public.commitments[0]), fr_to_hex(&public.commitments[1])],
            "ciphertexts": ["", ""],
            "withdraw_amount": "100000",
            "withdraw_to": alice.id(),
        }))
        .max_gas()
        .transact()
        .await?;
    assert!(
        r.is_failure() && failure_text(&r).contains("invalid proof"),
        "{}",
        failure_text(&r)
    );
    println!("all checks passed");
    Ok(())
}

#[tokio::test]
async fn swap_cycle() -> Result<()> {
    let chain = deploy(2).await?;
    let (pool, ft1, ft2, alice) = (
        &chain.pool,
        &chain.tokens[0],
        &chain.tokens[1],
        &chain.alice,
    );
    mint(ft1, alice, 1_600_000).await?;
    mint(ft2, alice, 1_000_000).await?;
    let pair_id: u32 = pool
        .call("register_pair")
        .args_json(json!({"asset_1": 1, "asset_2": 2}))
        .max_gas()
        .transact()
        .await?
        .into_result()?
        .json()?;
    assert_eq!(pair_id, 1);

    // 1. Public liquidity: 1_000_000 of each asset.
    for ft in [ft1, ft2] {
        let r = transfer_call(
            alice,
            ft,
            pool,
            1_000_000,
            json!({"liquidity_pair": pair_id}),
        )
        .await?;
        gas("add liquidity (ft_transfer_call)", &r);
        r.into_result()?;
    }
    let pair: Value = pool
        .view("get_pair")
        .args_json(json!({"pair_id": pair_id}))
        .await?
        .json()?;
    assert_eq!(pair["reserve_1"], "1000000");
    assert_eq!(pair["reserve_2"], "1000000");

    // 2. Alice deposits 600_000 of asset 1 into a note.
    let (js_pk, _) = keys::joinsplit_setup();
    let (sw_pk, _) = keys::swap_setup();
    let (sc_pk, _) = keys::swapclaim_setup();
    let mut wa = Wallet::random();
    let mut leaves: Vec<Fr> = vec![];
    let note_a = wa.make_note(1, 600_000);
    transfer_call(
        alice,
        ft1,
        pool,
        600_000,
        json!({"cm": fr_to_hex(&note_a.commitment()), "ciphertext": b64(&wa.seal_note(&note_a))}),
    )
    .await?
    .into_result()?;
    leaves.push(note_a.commitment());
    wa.scan(&fetch_leaves(pool).await?);

    // 3. Shielded swap: sell 100_000 of asset 1 in the current epoch, keep 500_000 as change.
    let epoch = epoch_with_room(&chain, 8).await?;
    let (a_note, a_pos) = wa.note_of(1, 600_000);
    let change = wa.make_note(1, 500_000);
    let swap = SwapRecord {
        epoch,
        pair_id,
        direction: DIR_1_TO_2,
        delta: 100_000,
        claim_pk: wa.pk(),
        rseed: random_fr(),
    };
    let inputs = [
        InputWitness {
            note: a_note,
            sk: wa.sk,
            position: a_pos,
            siblings: tree::merkle_path(&leaves, a_pos),
        },
        InputWitness::dummy(1, random_fr()),
    ];
    let circuit = SwapCircuit::new(tree::root_of(&leaves), 1, inputs, change, swap);
    let public = circuit.public.clone();
    let proof = keys::prove(&sw_pk, circuit, rand::random());
    let r = alice
        .call(pool.id(), "swap")
        .args_json(json!({
            "proof": b64(&near_encoding::proof_bytes(&proof)),
            "root": fr_to_hex(&public.root),
            "asset_id": 1,
            "nullifiers": [fr_to_hex(&public.nullifiers[0]), fr_to_hex(&public.nullifiers[1])],
            "change_commitment": fr_to_hex(&public.change_commitment),
            "change_ciphertext": b64(&wa.seal_note(&change)),
            "swap_commitment": fr_to_hex(&public.swap_commitment),
            "swap_ciphertext": b64(&wa.seal_swap(&swap)),
            "epoch": epoch,
            "pair_id": pair_id,
            "direction": DIR_1_TO_2,
            "delta": "100000",
        }))
        .max_gas()
        .transact()
        .await?;
    gas("swap", &r);
    r.into_result()?;
    leaves.extend([change.commitment(), swap.commitment()]);
    let batch: Value = pool
        .view("get_batch")
        .args_json(json!({"epoch": epoch, "pair_id": pair_id}))
        .await?
        .json()?;
    assert_eq!(batch["delta_1"], "100000");
    assert_eq!(batch["executed"], false);

    // 3b. Attacks while the epoch is still open (bogus proofs: the contract rejects them before verification).
    let (bogus_nf, bogus_cm) = (fr_to_hex(&random_fr()), fr_to_hex(&random_fr()));
    let bogus_proof = b64(&vec![0u8; near_encoding::PROOF_LEN]);
    let swap_args = |asset_id: u32, epoch_arg: u64| {
        json!({
            "proof": bogus_proof, "root": fr_to_hex(&tree::root_of(&leaves)), "asset_id": asset_id,
            "nullifiers": [bogus_nf, fr_to_hex(&random_fr())],
            "change_commitment": bogus_cm, "change_ciphertext": "", "swap_commitment": bogus_cm, "swap_ciphertext": "",
            "epoch": epoch_arg, "pair_id": pair_id, "direction": DIR_1_TO_2, "delta": "1",
        })
    };
    // Selling asset 2 while claiming direction 1 (asset 1 is the sold side).
    let r = alice
        .call(pool.id(), "swap")
        .args_json(swap_args(2, epoch))
        .max_gas()
        .transact()
        .await?;
    assert!(
        r.is_failure() && failure_text(&r).contains("does not match the pair side"),
        "{}",
        failure_text(&r)
    );
    // A swap declared for a future epoch.
    let r = alice
        .call(pool.id(), "swap")
        .args_json(swap_args(1, epoch + 1000))
        .max_gas()
        .transact()
        .await?;
    assert!(
        r.is_failure() && failure_text(&r).contains("future epoch"),
        "{}",
        failure_text(&r)
    );
    // Claiming an epoch that has not ended (far in the future, since the sandbox mints
    // blocks in milliseconds): the batch cannot be executed yet.
    let r = alice
        .call(pool.id(), "swap_claim")
        .args_json(json!({
            "proof": bogus_proof, "root": fr_to_hex(&tree::root_of(&leaves)), "nullifier": bogus_nf,
            "commitments": [bogus_cm, fr_to_hex(&random_fr())], "ciphertexts": ["", ""],
            "epoch": epoch + 1000, "pair_id": pair_id,
        }))
        .max_gas()
        .transact()
        .await?;
    assert!(
        r.is_failure() && failure_text(&r).contains("epoch not finished"),
        "{}",
        failure_text(&r)
    );

    // 4. The epoch ends; anyone executes the batch at one clearing price.
    chain.worker.fast_forward(EPOCH_BLOCKS + 1).await?;
    let r = chain
        .bob
        .call(pool.id(), "end_epoch")
        .args_json(json!({"epoch": epoch, "pair_id": pair_id}))
        .max_gas()
        .transact()
        .await?;
    gas("end_epoch", &r);
    let batch: Value = r.into_result()?.json()?;
    let (expected_out, _, _) = amm::execute(1_000_000, 1_000_000, 100_000);
    assert_eq!(expected_out, 90_661);
    assert_eq!(batch["lambda_2"], expected_out.to_string());
    assert_eq!(batch["unfilled_1"], "0");
    let bsod = Bsod {
        epoch,
        pair_id,
        asset_1: 1,
        asset_2: 2,
        delta_1: 100_000,
        delta_2: 0,
        lambda_1: 0,
        lambda_2: expected_out,
        unfilled_1: 0,
        unfilled_2: 0,
    };
    assert_eq!(batch["bsod_hash"], fr_to_hex(&bsod.hash()));

    // 5. Alice finds her swap by scanning and claims the result privately.
    wa.scan(&fetch_leaves(pool).await?);
    assert_eq!(wa.swaps.len(), 1);
    let (found, s_pos) = wa.swaps[0];
    assert_eq!((found, s_pos), (swap, 2));
    let claim = SwapClaimCircuit::new(
        tree::root_of(&leaves),
        swap,
        wa.sk,
        s_pos,
        tree::merkle_path(&leaves, s_pos),
        bsod,
        [random_fr(), random_fr()],
    );
    let outputs = claim.outputs;
    assert_eq!((outputs[0].asset_id, outputs[0].amount), (2, expected_out));
    assert_eq!((outputs[1].asset_id, outputs[1].amount), (1, 0));
    let public = claim.public.clone();
    let proof = keys::prove(&sc_pk, claim, rand::random());
    let r = alice
        .call(pool.id(), "swap_claim")
        .args_json(json!({
            "proof": b64(&near_encoding::proof_bytes(&proof)),
            "root": fr_to_hex(&public.root),
            "nullifier": fr_to_hex(&public.nullifier),
            "commitments": [fr_to_hex(&public.commitments[0]), fr_to_hex(&public.commitments[1])],
            "ciphertexts": [b64(&wa.seal_note(&outputs[0])), b64(&wa.seal_note(&outputs[1]))],
            "epoch": epoch,
            "pair_id": pair_id,
        }))
        .max_gas()
        .transact()
        .await?;
    gas("swap_claim", &r);
    r.into_result()?;
    leaves.extend([outputs[0].commitment(), outputs[1].commitment()]);

    // 5b. Attacks after the claim.
    // Replaying the same claim: the swap nullifier is spent.
    let r = alice
        .call(pool.id(), "swap_claim")
        .args_json(json!({
            "proof": b64(&near_encoding::proof_bytes(&proof)),
            "root": fr_to_hex(&public.root),
            "nullifier": fr_to_hex(&public.nullifier),
            "commitments": [fr_to_hex(&public.commitments[0]), fr_to_hex(&public.commitments[1])],
            "ciphertexts": ["", ""],
            "epoch": epoch, "pair_id": pair_id,
        }))
        .max_gas()
        .transact()
        .await?;
    assert!(
        r.is_failure() && failure_text(&r).contains("already spent"),
        "{}",
        failure_text(&r)
    );
    // A valid swap aimed at the executed epoch is refused: the batch is closed, and the call reverts entirely.
    wa.scan(&fetch_leaves(pool).await?);
    let (c_note, c_pos) = wa.note_of(1, 500_000);
    let late_inputs = [
        InputWitness {
            note: c_note,
            sk: wa.sk,
            position: c_pos,
            siblings: tree::merkle_path(&leaves, c_pos),
        },
        InputWitness::dummy(1, random_fr()),
    ];
    let late_change = wa.make_note(1, 400_000);
    let late_swap = SwapRecord {
        epoch,
        pair_id,
        direction: DIR_1_TO_2,
        delta: 100_000,
        claim_pk: wa.pk(),
        rseed: random_fr(),
    };
    let late = SwapCircuit::new(
        tree::root_of(&leaves),
        1,
        late_inputs,
        late_change,
        late_swap,
    );
    let late_public = late.public.clone();
    let late_proof = keys::prove(&sw_pk, late, rand::random());
    let r = alice
        .call(pool.id(), "swap")
        .args_json(json!({
            "proof": b64(&near_encoding::proof_bytes(&late_proof)),
            "root": fr_to_hex(&late_public.root),
            "asset_id": 1,
            "nullifiers": [fr_to_hex(&late_public.nullifiers[0]), fr_to_hex(&late_public.nullifiers[1])],
            "change_commitment": fr_to_hex(&late_public.change_commitment),
            "change_ciphertext": b64(&wa.seal_note(&late_change)),
            "swap_commitment": fr_to_hex(&late_public.swap_commitment),
            "swap_ciphertext": b64(&wa.seal_swap(&late_swap)),
            "epoch": epoch, "pair_id": pair_id, "direction": DIR_1_TO_2, "delta": "100000",
        }))
        .max_gas()
        .transact()
        .await?;
    assert!(
        r.is_failure() && failure_text(&r).contains("batch already executed"),
        "{}",
        failure_text(&r)
    );
    assert_eq!(
        pool.view("leaf_count").await?.json::<u64>()?,
        leaves.len() as u64
    );
    assert!(!pool
        .view("is_spent")
        .args_json(json!({"nullifier": fr_to_hex(&late_public.nullifiers[0])}))
        .await?
        .json::<bool>()?);

    // 6. The bought asset is an ordinary shielded note: withdraw it.
    wa.scan(&fetch_leaves(pool).await?);
    let (bought, b_pos) = wa.note_of(2, expected_out);
    let inputs = [
        InputWitness {
            note: bought,
            sk: wa.sk,
            position: b_pos,
            siblings: tree::merkle_path(&leaves, b_pos),
        },
        InputWitness::dummy(2, random_fr()),
    ];
    let zero = Note::dummy(2, wa.pk(), random_fr());
    let r = joinsplit_call(
        alice,
        pool,
        &js_pk,
        &leaves,
        2,
        inputs,
        [zero, Note::dummy(2, wa.pk(), random_fr())],
        [wa.seal_note(&zero), vec![]],
        Some((expected_out, alice)),
    )
    .await?;
    gas("withdraw bought asset", &r);
    r.into_result()?;
    assert_eq!(balance(ft2, alice).await?, expected_out);
    let pair: Value = pool
        .view("get_pair")
        .args_json(json!({"pair_id": pair_id}))
        .await?
        .json()?;
    assert_eq!(pair["reserve_1"], "1100000");
    assert_eq!(pair["reserve_2"], (1_000_000 - expected_out).to_string());
    println!("swap cycle passed");
    Ok(())
}
