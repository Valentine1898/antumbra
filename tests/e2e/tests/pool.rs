//! Deposit -> private transfer -> scan -> withdraw, on a local NEAR sandbox.
//!
//! Run: scripts/build-contracts.sh && CARGO_TARGET_DIR=target/tests cargo test -p antumbra-e2e -- --nocapture

use antumbra_circuits::{
    joinsplit::{InputWitness, JoinSplitCircuit},
    keys, near_encoding,
};
use antumbra_core::{
    crypto::{self, EncryptionKey, NotePlaintext},
    encoding::{fr_from_hex, fr_to_hex, negative_amount},
    note::{withdraw_ext_hash, Note, SpendKey},
    tree, Fr,
};
use anyhow::{anyhow, Result};
use ark_ff::{UniformRand, Zero};
use base64::Engine;
use near_workspaces::{result::ExecutionFinalResult, Account, Contract};
use serde_json::json;

const TGAS: f64 = 1e12;
const ASSET: u32 = 1;

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

fn gas(label: &str, r: &ExecutionFinalResult) {
    let receipt = r
        .receipt_outcomes()
        .first()
        .map(|o| o.gas_burnt.as_gas())
        .unwrap_or(0) as f64
        / TGAS;
    let total = r.total_gas_burnt.as_gas() as f64 / TGAS;
    println!(
        "{label:<40} receipt={receipt:>7.2} Tgas  total={total:>7.2} Tgas  logs={:?}",
        r.logs()
    );
    if r.is_failure() {
        println!("    FAILURES: {:?}", r.failures());
    }
}

struct Wallet {
    sk: SpendKey,
    enc: EncryptionKey,
    notes: Vec<(Note, u64)>, // note, position
}

impl Wallet {
    fn random() -> Self {
        let mut rng = ark_std::rand::rngs::StdRng::from_seed(rand::random());
        Wallet {
            sk: SpendKey(Fr::rand(&mut rng)),
            enc: EncryptionKey::random(),
            notes: vec![],
        }
    }
    fn pk(&self) -> Fr {
        self.sk.pk()
    }
    fn make_note(&self, amount: u128) -> Note {
        Note {
            asset_id: ASSET,
            amount,
            pk: self.pk(),
            rseed: random_fr(),
        }
    }
    /// Encrypts a note to this wallet's delivery key.
    fn seal(&self, note: &Note) -> Vec<u8> {
        let pt = NotePlaintext {
            asset_id: note.asset_id,
            amount: note.amount,
            rseed: note.rseed,
        };
        crypto::encrypt(&self.enc.public, &pt.to_bytes())
    }
    /// Trial-decrypts every leaf and keeps the ones locked to this wallet.
    fn scan(&mut self, leaves: &[(u64, Fr, Vec<u8>)]) {
        for (position, cm, blob) in leaves {
            if self.notes.iter().any(|(_, p)| p == position) {
                continue;
            }
            if let Some(pt) =
                crypto::decrypt(&self.enc.secret, blob).and_then(|b| NotePlaintext::from_bytes(&b))
            {
                let note = Note {
                    asset_id: pt.asset_id,
                    amount: pt.amount,
                    pk: self.pk(),
                    rseed: pt.rseed,
                };
                if note.commitment() == *cm {
                    self.notes.push((note, *position));
                }
            }
        }
    }
}

use ark_std::rand::SeedableRng;

fn random_fr() -> Fr {
    let mut rng = ark_std::rand::rngs::StdRng::from_seed(rand::random());
    Fr::rand(&mut rng)
}

async fn fetch_leaves(pool: &Contract) -> Result<Vec<(u64, Fr, Vec<u8>)>> {
    let count: u64 = pool.view("leaf_count").await?.json()?;
    let views: Vec<serde_json::Value> = pool
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

#[allow(clippy::too_many_arguments)]
async fn joinsplit_call(
    caller: &Account,
    pool: &Contract,
    pk: &antumbra_circuits::ProvingKey<antumbra_circuits::Bls12_381>,
    leaves: &[Fr],
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
    let circuit = JoinSplitCircuit::new(root, ASSET, inputs, outputs, public_amount, ext_hash);
    let public = circuit.public.clone();
    let proof = keys::prove(pk, circuit, rand::random());
    let mut args = json!({
        "proof": b64(&near_encoding::proof_bytes(&proof)),
        "root": fr_to_hex(&public.root),
        "asset_id": ASSET,
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
    let worker = near_workspaces::sandbox().await?;
    let ft = worker.dev_deploy(&wasm("test_ft")?).await?;
    ft.call("new").max_gas().transact().await?.into_result()?;
    let pool = worker.dev_deploy(&wasm("antumbra_pool")?).await?;
    pool.call("new")
        .args_json(json!({"owner": pool.id()}))
        .max_gas()
        .transact()
        .await?
        .into_result()?;
    let alice = worker.dev_create_account().await?;
    let bob = worker.dev_create_account().await?;

    // Asset registration on both sides.
    let r = pool
        .call("register_asset")
        .args_json(json!({"token": ft.id()}))
        .max_gas()
        .transact()
        .await?;
    assert_eq!(r.into_result()?.json::<u32>()?, ASSET);
    let deposit = near_workspaces::types::NearToken::from_millinear(10);
    for who in [pool.id(), bob.id()] {
        alice
            .call(ft.id(), "storage_deposit")
            .args_json(json!({"account_id": who}))
            .deposit(deposit)
            .max_gas()
            .transact()
            .await?
            .into_result()?;
    }
    ft.call("mint")
        .args_json(json!({"account_id": alice.id(), "amount": "1000000"}))
        .max_gas()
        .transact()
        .await?
        .into_result()?;

    for n in [0u32, 1, 10] {
        gas(
            &format!("bench_hash n={n}"),
            &pool
                .call("bench_hash")
                .args_json(json!({"n": n}))
                .max_gas()
                .transact()
                .await?,
        );
    }

    let (pk, _vk) = keys::joinsplit_setup();
    let mut wa = Wallet::random();
    let mut wb = Wallet::random();
    let mut leaves: Vec<Fr> = vec![];

    // 1. Alice deposits 600_000 into a note (public amount, hidden owner).
    let note_a = wa.make_note(600_000);
    let msg = json!({"cm": fr_to_hex(&note_a.commitment()), "ciphertext": b64(&wa.seal(&note_a))})
        .to_string();
    let r = alice
        .call(ft.id(), "ft_transfer_call")
        .args_json(json!({"receiver_id": pool.id(), "amount": "600000", "msg": msg}))
        .deposit(near_workspaces::types::NearToken::from_yoctonear(1))
        .max_gas()
        .transact()
        .await?;
    gas("deposit (ft_transfer_call)", &r);
    r.into_result()?;
    leaves.push(note_a.commitment());
    assert_eq!(pool.view("leaf_count").await?.json::<u64>()?, 1);
    assert_eq!(
        ft.view("ft_balance_of")
            .args_json(json!({"account_id": pool.id()}))
            .await?
            .json::<String>()?,
        "600000"
    );
    assert_eq!(
        pool.view("get_root").await?.json::<String>()?,
        fr_to_hex(&tree::root_of(&leaves))
    );
    wa.scan(&fetch_leaves(&pool).await?);
    assert_eq!(wa.notes.len(), 1);

    // 2. Alice privately sends 250_000 to Bob, keeps 350_000.
    let (a_note, a_pos) = wa.notes[0];
    let to_bob = wb.make_note(250_000);
    let change = wa.make_note(350_000);
    let inputs = [
        InputWitness {
            note: a_note,
            sk: wa.sk,
            position: a_pos,
            siblings: tree::merkle_path(&leaves, a_pos),
        },
        InputWitness::dummy(ASSET, random_fr()),
    ];
    let nf_a = inputs[0].nullifier();
    let r = joinsplit_call(
        &alice,
        &pool,
        &pk,
        &leaves,
        inputs,
        [to_bob, change],
        [wb.seal(&to_bob), wa.seal(&change)],
        None,
    )
    .await?;
    gas("joinsplit transfer", &r);
    r.into_result()?;
    leaves.push(to_bob.commitment());
    leaves.push(change.commitment());
    assert!(pool
        .view("is_spent")
        .args_json(json!({"nullifier": fr_to_hex(&nf_a)}))
        .await?
        .json::<bool>()?);
    assert_eq!(pool.view("leaf_count").await?.json::<u64>()?, 3);

    // 3. Bob scans the ciphertext log and finds exactly his note.
    let chain_leaves = fetch_leaves(&pool).await?;
    wb.scan(&chain_leaves);
    assert_eq!(wb.notes.len(), 1);
    assert_eq!(wb.notes[0].0.amount, 250_000);
    assert_eq!(wb.notes[0].1, 1);
    wa.scan(&chain_leaves);
    assert_eq!(wa.notes.len(), 2);

    // 4. Bob withdraws 100_000 to his NEAR account; change 150_000 stays shielded.
    let (b_note, b_pos) = wb.notes[0];
    let b_change = wb.make_note(150_000);
    let zero_out = Note::dummy(ASSET, wb.pk(), random_fr());
    let inputs = [
        InputWitness {
            note: b_note,
            sk: wb.sk,
            position: b_pos,
            siblings: tree::merkle_path(&leaves, b_pos),
        },
        InputWitness::dummy(ASSET, random_fr()),
    ];
    let r = joinsplit_call(
        &bob,
        &pool,
        &pk,
        &leaves,
        inputs.clone(),
        [b_change, zero_out],
        [wb.seal(&b_change), wb.seal(&zero_out)],
        Some((100_000, &bob)),
    )
    .await?;
    gas("joinsplit withdraw", &r);
    r.into_result()?;
    leaves.push(b_change.commitment());
    leaves.push(zero_out.commitment());
    assert_eq!(
        ft.view("ft_balance_of")
            .args_json(json!({"account_id": bob.id()}))
            .await?
            .json::<String>()?,
        "100000"
    );
    assert_eq!(
        ft.view("ft_balance_of")
            .args_json(json!({"account_id": pool.id()}))
            .await?
            .json::<String>()?,
        "500000"
    );

    // 5. Replaying the spent note fails, and a withdrawal proof bound to Bob cannot pay Alice.
    let r = joinsplit_call(
        &bob,
        &pool,
        &pk,
        &leaves[..3],
        inputs.clone(),
        [b_change, zero_out],
        [vec![], vec![]],
        Some((100_000, &bob)),
    )
    .await?;
    assert!(r.is_failure(), "double spend must fail");
    assert!(format!("{:?}", r.failures()).contains("already spent"));

    // Fresh proof for Bob's change note, but the contract is asked to pay Alice: ext_hash mismatch.
    let (c_note, c_pos) = (b_change, 3u64);
    let inputs = [
        InputWitness {
            note: c_note,
            sk: wb.sk,
            position: c_pos,
            siblings: tree::merkle_path(&leaves, c_pos),
        },
        InputWitness::dummy(ASSET, random_fr()),
    ];
    let circuit = JoinSplitCircuit::new(
        tree::root_of(&leaves),
        ASSET,
        inputs,
        [
            wb.make_note(50_000),
            Note::dummy(ASSET, wb.pk(), random_fr()),
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
            "asset_id": ASSET,
            "nullifiers": [fr_to_hex(&public.nullifiers[0]), fr_to_hex(&public.nullifiers[1])],
            "commitments": [fr_to_hex(&public.commitments[0]), fr_to_hex(&public.commitments[1])],
            "ciphertexts": ["", ""],
            "withdraw_amount": "100000",
            "withdraw_to": alice.id(),
        }))
        .max_gas()
        .transact()
        .await?;
    assert!(r.is_failure(), "proof bound to bob must not pay alice");
    assert!(format!("{:?}", r.failures()).contains("invalid proof"));
    println!("all checks passed");
    Ok(())
}
