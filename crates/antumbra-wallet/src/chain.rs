//! Thin client over near-workspaces: one signing account, one pool contract.

use anyhow::{anyhow, Result};
use near_workspaces::{
    result::ExecutionFinalResult,
    types::{NearToken, SecretKey},
    Account, AccountId,
};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::{json, Value};
use std::str::FromStr;

pub struct Chain {
    pub account: Account,
    pub pool: AccountId,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PairInfo {
    pub pair_id: u32,
    pub asset_1: u32,
    pub asset_2: u32,
    pub reserve_1: String,
    pub reserve_2: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct EpochInfo {
    pub epoch: u64,
    pub block_height: u64,
    pub blocks_left: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchInfo {
    pub epoch: u64,
    pub pair_id: u32,
    pub executed: bool,
    pub delta_1: String,
    pub delta_2: String,
    pub lambda_1: String,
    pub lambda_2: String,
    pub unfilled_1: String,
    pub unfilled_2: String,
    pub bsod_hash: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LeafInfo {
    pub position: u64,
    pub cm: String,
    pub ciphertext: String,
}

impl Chain {
    /// `network` is `testnet` or an RPC url. `rpc` overrides testnet's default RPC.
    pub async fn connect(
        network: &str,
        rpc: Option<&str>,
        account_id: &str,
        secret_key: &str,
        pool: &str,
    ) -> Result<Self> {
        let sk = SecretKey::from_str(secret_key).map_err(|e| anyhow!("bad secret key: {e}"))?;
        let id: AccountId = account_id.parse()?;
        // The testnet builder is just "an RPC endpoint" for signing and querying,
        // so any RPC url (a local sandbox included) goes through it.
        let rpc_url = match (network, rpc) {
            ("testnet", Some(url)) => Some(url.to_string()),
            ("testnet", None) => None,
            (url, _) if url.starts_with("http") => Some(url.to_string()),
            (other, _) => {
                return Err(anyhow!(
                    "unknown network {other}: use `testnet` or an RPC url"
                ))
            }
        };
        let account = match rpc_url {
            Some(url) => {
                Account::from_secret_key(id, sk, &near_workspaces::testnet().rpc_addr(&url).await?)
            }
            None => Account::from_secret_key(id, sk, &near_workspaces::testnet().await?),
        };
        Ok(Chain {
            account,
            pool: pool.parse()?,
        })
    }

    pub async fn view<T: DeserializeOwned>(&self, method: &str, args: Value) -> Result<T> {
        Ok(self
            .account
            .view(&self.pool, method)
            .args_json(args)
            .await?
            .json()?)
    }

    pub async fn call(&self, method: &str, args: Value) -> Result<ExecutionFinalResult> {
        Ok(self
            .account
            .call(&self.pool, method)
            .args_json(args)
            .max_gas()
            .transact()
            .await?)
    }

    pub async fn ft_transfer_call(
        &self,
        token: &AccountId,
        amount: u128,
        msg: Value,
    ) -> Result<ExecutionFinalResult> {
        Ok(self
            .account
            .call(token, "ft_transfer_call")
            .args_json(json!({"receiver_id": self.pool, "amount": amount.to_string(), "msg": msg.to_string()}))
            .deposit(NearToken::from_yoctonear(1))
            .max_gas()
            .transact()
            .await?)
    }

    pub async fn ft_balance(&self, token: &AccountId, who: &AccountId) -> Result<u128> {
        let s: String = self
            .account
            .view(token, "ft_balance_of")
            .args_json(json!({"account_id": who}))
            .await?
            .json()?;
        Ok(s.parse()?)
    }

    pub async fn leaf_count(&self) -> Result<u64> {
        self.view("leaf_count", json!({})).await
    }

    pub async fn get_root(&self) -> Result<String> {
        self.view("get_root", json!({})).await
    }

    pub async fn get_leaves(&self, from: u64, limit: u64) -> Result<Vec<LeafInfo>> {
        self.view("get_leaves", json!({"from": from, "limit": limit}))
            .await
    }

    pub async fn get_pair(&self, pair_id: u32) -> Result<Option<PairInfo>> {
        self.view("get_pair", json!({"pair_id": pair_id})).await
    }

    pub async fn current_epoch(&self) -> Result<EpochInfo> {
        self.view("current_epoch", json!({})).await
    }

    pub async fn get_batch(&self, epoch: u64, pair_id: u32) -> Result<Option<BatchInfo>> {
        self.view("get_batch", json!({"epoch": epoch, "pair_id": pair_id}))
            .await
    }

    pub async fn is_spent(&self, nullifier_hex: &str) -> Result<bool> {
        self.view("is_spent", json!({"nullifier": nullifier_hex}))
            .await
    }

    pub async fn get_token(&self, asset_id: u32) -> Result<Option<AccountId>> {
        self.view("get_token", json!({"asset_id": asset_id})).await
    }

    pub async fn get_asset_id(&self, token: &AccountId) -> Result<Option<u32>> {
        self.view("get_asset_id", json!({"token": token})).await
    }
}

/// One-line summary of a transaction: gas, logs, failure.
pub fn describe(r: &ExecutionFinalResult) -> String {
    let total = r.total_gas_burnt.as_gas() as f64 / 1e12;
    let logs: Vec<&str> = r
        .logs()
        .into_iter()
        .filter(|l| !l.starts_with("EVENT_JSON"))
        .collect();
    let mut s = format!("{total:.1} Tgas");
    if !logs.is_empty() {
        s.push_str(&format!(" | {}", logs.join(" | ")));
    }
    if r.is_failure() {
        s.push_str(&format!(" | FAILED: {:?}", r.failures()));
    }
    s
}

/// Errors out on a failed transaction, keeping the chain's message.
pub fn ensure_ok(r: ExecutionFinalResult) -> Result<ExecutionFinalResult> {
    if r.is_failure() {
        let msg = format!("{:?}", r.failures());
        return Err(anyhow!("transaction failed: {msg}"));
    }
    Ok(r)
}
