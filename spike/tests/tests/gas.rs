//! Deploys the spike contract to a local sandbox and prints a gas table.
//!
//! Run: cargo test -p spike-tests -- --nocapture
//! Needs: spike/fixtures/*.json (cargo run -p spike-gen --release) and the
//! contract wasm (cargo build -p spike-contract --target wasm32-unknown-unknown --release).

use anyhow::{anyhow, Result};
use base64::Engine;
use near_workspaces::{result::ExecutionFinalResult, Contract};
use serde_json::{json, Value};

const TGAS: f64 = 1e12;

fn fixture(name: &str) -> Result<Value> {
    let p = format!("{}/../fixtures/{name}.json", env!("CARGO_MANIFEST_DIR"));
    Ok(serde_json::from_str(
        &std::fs::read_to_string(&p).map_err(|e| anyhow!("{p}: {e}"))?,
    )?)
}

fn b64(bytes: Vec<u8>) -> String {
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

fn hx(v: &Value, key: &str) -> Vec<u8> {
    hex::decode(v[key].as_str().expect("hex string")).expect("valid hex")
}

async fn call(c: &Contract, method: &str, args: Value) -> Result<ExecutionFinalResult> {
    Ok(c.call(method).args_json(args).max_gas().transact().await?)
}

/// Prints one row and returns the JSON result when the call succeeded.
fn report(label: &str, r: ExecutionFinalResult) -> Option<Value> {
    let receipt = r
        .receipt_outcomes()
        .first()
        .map(|o| o.gas_burnt.as_gas())
        .unwrap_or(0) as f64
        / TGAS;
    let total = r.total_gas_burnt.as_gas() as f64 / TGAS;
    let logs = r.logs().join(" | ");
    match r.into_result() {
        Ok(ok) => {
            let v: Value = ok.json().unwrap_or(Value::Null);
            println!("{label:<36} receipt={receipt:>7.2} Tgas  total={total:>7.2} Tgas  result={v} {logs}");
            Some(v)
        }
        Err(e) => {
            let msg = format!("{e:?}");
            let msg: String = msg.chars().take(220).collect();
            println!(
                "{label:<36} receipt={receipt:>7.2} Tgas  total={total:>7.2} Tgas  FAILED: {msg}"
            );
            None
        }
    }
}

#[tokio::test]
async fn gas_table() -> Result<()> {
    let wasm_path = format!(
        "{}/../../target/wasm32-unknown-unknown/release/spike_contract.wasm",
        env!("CARGO_MANIFEST_DIR")
    );
    let wasm = std::fs::read(&wasm_path)
        .map_err(|e| anyhow!("{wasm_path}: {e} (build the contract first)"))?;
    let worker = near_workspaces::sandbox().await?;
    let contract = worker.dev_deploy(&wasm).await?;
    println!("\ncontract wasm: {} KB\n", wasm.len() / 1024);

    report("noop (baseline)", call(&contract, "noop", json!({})).await?);

    // Host-function paths: valid proof must pass, a changed public input must fail cleanly.
    for (curve, method) in [
        ("bn254", "verify_bn254_host"),
        ("bls12_381", "verify_bls12381_host"),
    ] {
        let f = fixture(curve)?;
        let near = &f["near"];
        let (vk, proof, inputs) = (hx(near, "vk"), hx(near, "proof"), hx(near, "inputs"));

        let args = json!({ "vk": b64(vk.clone()), "proof": b64(proof.clone()), "inputs": b64(inputs.clone()) });
        let v = report(
            &format!("{method} (valid)"),
            call(&contract, method, args).await?,
        );
        assert_eq!(v, Some(json!(true)), "{method}: valid proof must verify");

        let mut bad_inputs = inputs.clone();
        bad_inputs[0] ^= 0x01; // still a canonical scalar, just the wrong statement
        let args = json!({ "vk": b64(vk), "proof": b64(proof), "inputs": b64(bad_inputs) });
        let v = report(
            &format!("{method} (wrong input)"),
            call(&contract, method, args).await?,
        );
        assert_eq!(
            v,
            Some(json!(false)),
            "{method}: wrong statement must not verify"
        );
    }

    // Pure-wasm paths: may exceed 300 Tgas, so no assertion, only the row.
    for (curve, method) in [
        ("bn254", "verify_bn254_wasm"),
        ("bls12_377", "verify_bls12377_wasm"),
    ] {
        let f = fixture(curve)?;
        let ark = &f["ark"];
        let args = json!({ "pvk": b64(hx(ark, "pvk")), "proof": b64(hx(ark, "proof")), "inputs": b64(hx(ark, "inputs")) });
        report(
            &format!("{method} (valid)"),
            call(&contract, method, args).await?,
        );
    }

    for n in [1u32, 8, 32] {
        report(
            &format!("poseidon_bench n={n}"),
            call(&contract, "poseidon_bench", json!({ "n": n })).await?,
        );
    }
    println!();
    Ok(())
}
