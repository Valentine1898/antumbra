//! `antumbra`: a command-line wallet for the Antumbra shielded pool, plus a
//! testnet deployer for demos.

use antumbra_wallet::{
    chain::{describe, ensure_ok},
    ops::{self, ClaimOutcome},
    Address, Chain, ProvingKeys, WalletFile,
};
use anyhow::{anyhow, Context, Result};
use clap::{Parser, Subcommand};
use near_workspaces::{types::NearToken, Account, Contract};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Parser)]
#[command(
    name = "antumbra",
    version,
    about = "Shielded pool with batch swaps on NEAR"
)]
struct Cli {
    /// Wallet file (created by `keys-new`).
    #[arg(
        long,
        global = true,
        env = "ANTUMBRA_WALLET",
        default_value = "antumbra-wallet.json"
    )]
    wallet: PathBuf,
    /// `testnet` or an RPC url.
    #[arg(
        long,
        global = true,
        env = "ANTUMBRA_NETWORK",
        default_value = "testnet"
    )]
    network: String,
    /// RPC override for testnet (for example https://test.rpc.fastnear.com).
    #[arg(long, global = true, env = "ANTUMBRA_RPC")]
    rpc: Option<String>,
    /// NEAR account that signs and pays for transactions.
    #[arg(long, global = true, env = "ANTUMBRA_ACCOUNT")]
    account: Option<String>,
    /// Its secret key (ed25519:...).
    #[arg(long, global = true, env = "ANTUMBRA_KEY")]
    key: Option<String>,
    /// Pool contract account id.
    #[arg(long, global = true, env = "ANTUMBRA_POOL")]
    pool: Option<String>,
    /// Deployment file written by `deploy-testnet` (fills account, key and pool when flags are absent).
    #[arg(
        long,
        global = true,
        env = "ANTUMBRA_DEPLOYMENT",
        default_value = "deployment.json"
    )]
    deployment: PathBuf,
    /// Directory for cached proving keys.
    #[arg(
        long,
        global = true,
        env = "ANTUMBRA_PROVING_KEYS",
        default_value = "proving-keys"
    )]
    proving_keys: PathBuf,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Create a new wallet file with fresh keys.
    KeysNew,
    /// Print this wallet's shielded address.
    Address,
    /// Scan the pool for new notes and swaps, refresh spent flags, print balances.
    Sync,
    /// Print balances from the local wallet state.
    Balance,
    /// Pool state: root, leaves, epoch, pairs.
    Info,
    /// Deposit tokens into a fresh shielded note.
    Deposit {
        #[arg(long)]
        token: String,
        #[arg(long)]
        amount: u128,
    },
    /// Private transfer to a shielded address.
    Transfer {
        #[arg(long)]
        asset: u32,
        #[arg(long)]
        amount: u128,
        #[arg(long)]
        to: String,
    },
    /// Withdraw shielded tokens to a NEAR account.
    Withdraw {
        #[arg(long)]
        asset: u32,
        #[arg(long)]
        amount: u128,
        #[arg(long)]
        to: String,
    },
    /// Shielded swap in the current epoch.
    Swap {
        #[arg(long)]
        pair: u32,
        #[arg(long)]
        sell: u32,
        #[arg(long)]
        amount: u128,
    },
    /// Claim every cleared swap into private notes.
    Claim,
    /// The sealer: opens sealed swap amounts and clears epochs with signed totals.
    Sealer {
        #[command(subcommand)]
        cmd: SealerCmd,
    },
    /// Deploy the pool and test tokens, seed liquidity, write deployment.json.
    /// With --account/--key the contracts go to fresh subaccounts of that funded
    /// account (works on testnet and on a local sandbox); without them a testnet
    /// dev account is created through the helper service.
    Deploy {
        #[arg(long, default_value_t = 2)]
        tokens: u32,
        #[arg(long, default_value = "target/wasm32-unknown-unknown/release")]
        wasm_dir: PathBuf,
        /// Tokens minted to the deployer per test token.
        #[arg(long, default_value_t = 10_000_000)]
        mint: u128,
        /// Liquidity seeded per side of the pair.
        #[arg(long, default_value_t = 1_000_000)]
        liquidity: u128,
    },
    /// Mint test tokens (test tokens only).
    Mint {
        #[arg(long)]
        token: String,
        #[arg(long)]
        to: String,
        #[arg(long)]
        amount: u128,
    },
    /// Add public liquidity to a pair.
    AddLiquidity {
        #[arg(long)]
        pair: u32,
        #[arg(long)]
        token: String,
        #[arg(long)]
        amount: u128,
    },
}

#[derive(Serialize, Deserialize, Default)]
struct Deployment {
    network: String,
    pool: String,
    tokens: Vec<String>,
    pair_id: u32,
    deployer: String,
}

#[derive(Subcommand)]
enum SealerCmd {
    /// Print the sealer's public key and the executor's public key.
    Pubkey,
    /// Register the sealer keys on the pool (signs as the pool account).
    Register {
        /// OutLayer request contract to clear epochs inside a TEE (optional).
        #[arg(long)]
        outlayer_account: Option<String>,
        #[arg(long)]
        wasm_url: Option<String>,
        #[arg(long)]
        wasm_hash: Option<String>,
        #[arg(long)]
        secrets_profile: Option<String>,
    },
    /// Open and clear a finished epoch (default: every uncleared one in the lookback window).
    Clear {
        #[arg(long)]
        pair: u32,
        #[arg(long)]
        epoch: Option<u64>,
        #[arg(long, default_value_t = 50)]
        lookback: u64,
    },
    /// Keep clearing finished epochs as they end. Each tick is one view call
    /// per epoch in the lookback window, so keep the window small on public RPCs.
    Watch {
        #[arg(long)]
        pair: u32,
        #[arg(long, default_value_t = 15)]
        interval_secs: u64,
        #[arg(long, default_value_t = 5)]
        lookback: u64,
    },
}

#[derive(Serialize, Deserialize)]
struct Secrets {
    deployer_account: String,
    deployer_key: String,
    pool_key: String,
    /// Sealer secret (hex) and executor signing key seed (hex): the signed-executor path.
    #[serde(default)]
    sealer_seed: Option<String>,
    #[serde(default)]
    executor_key: Option<String>,
}

fn random_hex32() -> String {
    hex::encode(rand::random::<[u8; 32]>())
}

fn sealer_secrets(sec: &Secrets) -> Result<ops::SealerSecrets> {
    let seed = hex::decode(sec.sealer_seed.as_ref().ok_or_else(|| {
        anyhow!("no sealer_seed in the secret file; run `antumbra sealer keygen` or `deploy`")
    })?)?;
    let ek: [u8; 32] = hex::decode(
        sec.executor_key
            .as_ref()
            .ok_or_else(|| anyhow!("no executor_key in the secret file"))?,
    )?
    .try_into()
    .map_err(|_| anyhow!("executor_key must be 32 bytes"))?;
    Ok(ops::SealerSecrets {
        seed,
        executor: ed25519_dalek::SigningKey::from_bytes(&ek),
    })
}

fn new_sealer_secrets() -> (String, String) {
    (random_hex32(), random_hex32())
}

fn load_deployment(path: &Path) -> Option<Deployment> {
    fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
}

fn load_secrets(path: &Path) -> Option<Secrets> {
    let secret_path = path.with_extension("secret.json");
    fs::read_to_string(secret_path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
}

impl Cli {
    fn resolved(&self) -> Result<(String, String, String)> {
        let dep = load_deployment(&self.deployment);
        let sec = load_secrets(&self.deployment);
        let account = self
            .account
            .clone()
            .or_else(|| sec.as_ref().map(|s| s.deployer_account.clone()))
            .ok_or_else(|| anyhow!("--account (or deployment.secret.json) required"))?;
        let key = self
            .key
            .clone()
            .or_else(|| sec.as_ref().map(|s| s.deployer_key.clone()))
            .ok_or_else(|| anyhow!("--key (or deployment.secret.json) required"))?;
        let pool = self
            .pool
            .clone()
            .or_else(|| dep.as_ref().map(|d| d.pool.clone()))
            .ok_or_else(|| anyhow!("--pool (or deployment.json) required"))?;
        Ok((account, key, pool))
    }

    async fn chain(&self) -> Result<Chain> {
        let (account, key, pool) = self.resolved()?;
        Chain::connect(&self.network, self.rpc.as_deref(), &account, &key, &pool).await
    }

    fn wallet(&self) -> Result<WalletFile> {
        WalletFile::load(&self.wallet).with_context(|| "no wallet; run `antumbra keys-new` first")
    }
}

fn print_balances(wallet: &WalletFile) {
    let balances = wallet.balances();
    if balances.is_empty() {
        println!("shielded balance: empty");
    }
    for (asset, amount) in balances {
        println!("shielded balance: asset {asset} = {amount}");
    }
    let pending: Vec<_> = wallet.swaps.iter().filter(|s| !s.claimed).collect();
    for s in pending {
        println!(
            "pending swap: epoch {} pair {} direction {} delta {} (position {})",
            s.epoch, s.pair_id, s.direction, s.delta, s.position
        );
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    match &cli.cmd {
        Cmd::KeysNew => {
            if cli.wallet.exists() {
                return Err(anyhow!("{} already exists", cli.wallet.display()));
            }
            let w = WalletFile::generate();
            w.save(&cli.wallet)?;
            println!("wallet written to {}", cli.wallet.display());
            println!("address: {}", w.address().encode());
        }
        Cmd::Address => println!("{}", cli.wallet()?.address().encode()),
        Cmd::Balance => print_balances(&cli.wallet()?),
        Cmd::Sync => {
            let mut w = cli.wallet()?;
            let chain = cli.chain().await?;
            let r = ops::sync(&mut w, &chain).await?;
            w.save(&cli.wallet)?;
            println!("synced: {} new leaves, {} new notes, {} new swaps, {} newly spent, {} newly claimed", r.new_leaves, r.new_notes, r.new_swaps, r.newly_spent, r.newly_claimed);
            print_balances(&w);
        }
        Cmd::Info => {
            let chain = cli.chain().await?;
            println!("pool: {}", chain.pool);
            println!("root: {}", chain.get_root().await?);
            println!("leaves: {}", chain.leaf_count().await?);
            let e = chain.current_epoch().await?;
            println!(
                "epoch: {} (block {}, {} blocks left)",
                e.epoch, e.block_height, e.blocks_left
            );
            let mut pair_id = 1;
            while let Some(p) = chain.get_pair(pair_id).await? {
                let t1 = chain
                    .get_token(p.asset_1)
                    .await?
                    .map(|a| a.to_string())
                    .unwrap_or_default();
                let t2 = chain
                    .get_token(p.asset_2)
                    .await?
                    .map(|a| a.to_string())
                    .unwrap_or_default();
                println!(
                    "pair {}: asset {} ({t1}) reserve {} / asset {} ({t2}) reserve {}",
                    p.pair_id, p.asset_1, p.reserve_1, p.asset_2, p.reserve_2
                );
                if let Some(b) = chain.get_batch(e.epoch, pair_id).await? {
                    println!(
                        "  current batch: delta_1 {} delta_2 {}",
                        b.delta_1, b.delta_2
                    );
                }
                pair_id += 1;
            }
        }
        Cmd::Deposit { token, amount } => {
            let w = cli.wallet()?;
            let chain = cli.chain().await?;
            let r = ops::deposit(&w, &chain, &token.parse()?, *amount).await?;
            println!("deposit ok: {}", describe(&r));
            println!("run `antumbra sync` to pick up the new note");
        }
        Cmd::Transfer { asset, amount, to } => {
            let mut w = cli.wallet()?;
            let chain = cli.chain().await?;
            let keys = ProvingKeys::load_or_generate(&cli.proving_keys)?;
            let to = Address::parse(to)?;
            let r = ops::transfer(&mut w, &chain, &keys, *asset, *amount, to).await?;
            w.save(&cli.wallet)?;
            println!("transfer ok: {}", describe(&r));
        }
        Cmd::Withdraw { asset, amount, to } => {
            let mut w = cli.wallet()?;
            let chain = cli.chain().await?;
            let keys = ProvingKeys::load_or_generate(&cli.proving_keys)?;
            let r = ops::withdraw(&mut w, &chain, &keys, *asset, *amount, &to.parse()?).await?;
            w.save(&cli.wallet)?;
            println!("withdraw ok: {}", describe(&r));
        }
        Cmd::Swap { pair, sell, amount } => {
            let mut w = cli.wallet()?;
            let chain = cli.chain().await?;
            let keys = ProvingKeys::load_or_generate(&cli.proving_keys)?;
            let (epoch, r) = ops::swap(&mut w, &chain, &keys, *pair, *sell, *amount).await?;
            w.save(&cli.wallet)?;
            println!("swap submitted for epoch {epoch}: {}", describe(&r));
            println!("after the epoch ends the sealer clears it (`antumbra sealer clear --pair {pair}`), then `antumbra claim`");
        }
        Cmd::Sealer { cmd } => sealer_cmd(&cli, cmd).await?,
        Cmd::Claim => {
            let mut w = cli.wallet()?;
            let chain = cli.chain().await?;
            let keys = ProvingKeys::load_or_generate(&cli.proving_keys)?;
            ops::sync(&mut w, &chain).await?;
            let outcomes = ops::claim_all(&mut w, &chain, &keys).await?;
            w.save(&cli.wallet)?;
            if outcomes.is_empty() {
                println!("nothing to claim");
            }
            for o in outcomes {
                match o {
                    ClaimOutcome::Claimed {
                        index,
                        bought_asset,
                        bought,
                        refund,
                        summary,
                    } => {
                        println!("swap #{index} claimed: {bought} of asset {bought_asset} (refund {refund}) | {summary}")
                    }
                    ClaimOutcome::Waiting {
                        index,
                        epoch,
                        blocks_left,
                    } => println!(
                        "swap #{index} waits for epoch {epoch} to end (~{blocks_left} blocks)"
                    ),
                    ClaimOutcome::NotCleared { index, epoch } => println!(
                        "swap #{index}: epoch {epoch} has ended but the sealer has not cleared it yet (run `antumbra sealer clear --pair <id>`)"
                    ),
                }
            }
            println!("run `antumbra sync` to pick up the claimed notes");
        }
        Cmd::Mint { token, to, amount } => {
            let chain = cli.chain().await?;
            let r = ensure_ok(
                chain
                    .account
                    .call(&token.parse()?, "mint")
                    .args_json(json!({"account_id": to, "amount": amount.to_string()}))
                    .max_gas()
                    .transact()
                    .await?,
            )?;
            println!("mint ok: {}", describe(&r));
        }
        Cmd::AddLiquidity {
            pair,
            token,
            amount,
        } => {
            let chain = cli.chain().await?;
            let r = ensure_ok(
                chain
                    .ft_transfer_call(&token.parse()?, *amount, json!({"liquidity_pair": pair}))
                    .await?,
            )?;
            println!("liquidity ok: {}", describe(&r));
        }
        Cmd::Deploy {
            tokens,
            wasm_dir,
            mint,
            liquidity,
        } => deploy(&cli, *tokens, wasm_dir, *mint, *liquidity).await?,
    }
    Ok(())
}

async fn deploy(
    cli: &Cli,
    n_tokens: u32,
    wasm_dir: &Path,
    mint: u128,
    liquidity: u128,
) -> Result<()> {
    let pool_wasm = fs::read(wasm_dir.join("antumbra_pool.wasm"))
        .with_context(|| "pool wasm missing; run scripts/build-contracts.sh")?;
    let ft_wasm = fs::read(wasm_dir.join("test_ft.wasm"))
        .with_context(|| "test_ft wasm missing; run scripts/build-contracts.sh")?;
    let rpc_url = match (cli.network.as_str(), cli.rpc.as_deref()) {
        ("testnet", Some(url)) => Some(url.to_string()),
        ("testnet", None) => None,
        (url, _) if url.starts_with("http") => Some(url.to_string()),
        (other, _) => return Err(anyhow!("unknown network {other}")),
    };
    let worker = match &rpc_url {
        Some(url) => near_workspaces::testnet().rpc_addr(url).await?,
        None => near_workspaces::testnet().await?,
    };
    let suffix: String = format!("{:06x}", rand_suffix());
    let (deployer, pool, fts): (Account, Contract, Vec<Contract>) = match (&cli.account, &cli.key) {
        (Some(id), Some(key)) => {
            let root = Account::from_secret_key(
                id.parse()?,
                key.parse().map_err(|e| anyhow!("bad key: {e}"))?,
                &worker,
            );
            println!("deploying as subaccounts of {}", root.id());
            let pool_acc = root
                .create_subaccount(&format!("pool-{suffix}"))
                .initial_balance(NearToken::from_near(6))
                .transact()
                .await?
                .into_result()?;
            let pool = pool_acc.deploy(&pool_wasm).await?.into_result()?;
            let mut fts = vec![];
            for i in 0..n_tokens {
                let acc = root
                    .create_subaccount(&format!("ft{}-{suffix}", i + 1))
                    .initial_balance(NearToken::from_near(3))
                    .transact()
                    .await?
                    .into_result()?;
                fts.push(acc.deploy(&ft_wasm).await?.into_result()?);
            }
            (root, pool, fts)
        }
        (None, None) if cli.network == "testnet" => {
            println!("creating a testnet dev account through the helper service...");
            let deployer = worker.dev_create_account().await?;
            let pool = worker.dev_deploy(&pool_wasm).await?;
            let mut fts = vec![];
            for _ in 0..n_tokens {
                fts.push(worker.dev_deploy(&ft_wasm).await?);
            }
            (deployer, pool, fts)
        }
        _ => {
            return Err(anyhow!(
                "pass both --account and --key of a funded account (required on a local sandbox)"
            ))
        }
    };
    println!("deployer: {}", deployer.id());
    let _ = ensure_ok(
        pool.call("new")
            .args_json(json!({"owner": pool.id()}))
            .max_gas()
            .transact()
            .await?,
    )?;
    println!("pool: {}", pool.id());
    let mut token_ids = vec![];
    for (i, ft) in fts.iter().enumerate() {
        let _ = ensure_ok(ft.call("new").max_gas().transact().await?)?;
        let asset: u32 = ensure_ok(
            pool.call("register_asset")
                .args_json(json!({"token": ft.id()}))
                .max_gas()
                .transact()
                .await?,
        )?
        .json()?;
        let _ = ensure_ok(
            deployer
                .call(ft.id(), "storage_deposit")
                .args_json(json!({"account_id": pool.id()}))
                .deposit(NearToken::from_millinear(10))
                .max_gas()
                .transact()
                .await?,
        )?;
        let _ = ensure_ok(
            ft.call("mint")
                .args_json(json!({"account_id": deployer.id(), "amount": mint.to_string()}))
                .max_gas()
                .transact()
                .await?,
        )?;
        println!(
            "token {}: {} (asset {asset}), minted {mint} to deployer",
            i + 1,
            ft.id()
        );
        token_ids.push(ft.id().clone());
    }
    let mut pair_id = 0;
    if n_tokens >= 2 {
        pair_id = ensure_ok(
            pool.call("register_pair")
                .args_json(json!({"asset_1": 1, "asset_2": 2}))
                .max_gas()
                .transact()
                .await?,
        )?
        .json()?;
        for t in &token_ids[..2] {
            let _ = ensure_ok(
                deployer
                    .call(t, "ft_transfer_call")
                    .args_json(json!({"receiver_id": pool.id(), "amount": liquidity.to_string(), "msg": json!({"liquidity_pair": pair_id}).to_string()}))
                    .deposit(NearToken::from_yoctonear(1))
                    .max_gas()
                    .transact()
                    .await?,
            )?;
        }
        println!("pair {pair_id}: {liquidity} / {liquidity} liquidity seeded");
    }
    // The sealer keys: what swap amounts are encrypted to, and who may sign cleared totals.
    let (sealer_seed, executor_key) = new_sealer_secrets();
    let sealer = sealer_secrets(&Secrets {
        deployer_account: String::new(),
        deployer_key: String::new(),
        pool_key: String::new(),
        sealer_seed: Some(sealer_seed.clone()),
        executor_key: Some(executor_key.clone()),
    })?;
    let (px, py, epk) = sealer.public_parts();
    let _ = ensure_ok(
        pool.call("set_sealer")
            .args_json(json!({"pk": [px, py], "executor_pk": epk}))
            .max_gas()
            .transact()
            .await?,
    )?;
    println!("sealer registered (signed executor path); keep its secrets in the secret file");
    let deployment = Deployment {
        network: cli.network.clone(),
        pool: pool.id().to_string(),
        tokens: token_ids.iter().map(|t| t.to_string()).collect(),
        pair_id,
        deployer: deployer.id().to_string(),
    };
    fs::write(&cli.deployment, serde_json::to_string_pretty(&deployment)?)?;
    let secrets = Secrets {
        deployer_account: deployer.id().to_string(),
        deployer_key: deployer.secret_key().to_string(),
        pool_key: pool.as_account().secret_key().to_string(),
        sealer_seed: Some(sealer_seed),
        executor_key: Some(executor_key),
    };
    let secret_path = cli.deployment.with_extension("secret.json");
    fs::write(&secret_path, serde_json::to_string_pretty(&secrets)?)?;
    println!(
        "wrote {} and {} (keep the latter private)",
        cli.deployment.display(),
        secret_path.display()
    );
    Ok(())
}

fn rand_suffix() -> u32 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    (nanos ^ std::process::id()) & 0xff_ffff
}

async fn sealer_cmd(cli: &Cli, cmd: &SealerCmd) -> Result<()> {
    let sec = load_secrets(&cli.deployment)
        .ok_or_else(|| anyhow!("no secret file next to {}", cli.deployment.display()))?;
    let dep = load_deployment(&cli.deployment)
        .ok_or_else(|| anyhow!("no deployment file {}", cli.deployment.display()))?;
    match cmd {
        SealerCmd::Pubkey => {
            let (px, py, epk) = sealer_secrets(&sec)?.public_parts();
            println!("sealer pk: [{px}, {py}]");
            println!("executor pk: {epk}");
        }
        SealerCmd::Register {
            outlayer_account,
            wasm_url,
            wasm_hash,
            secrets_profile,
        } => {
            let (px, py, epk) = sealer_secrets(&sec)?.public_parts();
            // The pool account is its own owner, so registration signs with the pool key.
            let chain = Chain::connect(
                &cli.network,
                cli.rpc.as_deref(),
                &dep.pool,
                &sec.pool_key,
                &dep.pool,
            )
            .await?;
            let outlayer = match (outlayer_account, wasm_url, wasm_hash) {
                (Some(a), Some(u), Some(h)) => {
                    json!({"account": a, "wasm_url": u, "wasm_hash": h, "secrets_profile": secrets_profile})
                }
                (None, None, None) => json!(null),
                _ => {
                    return Err(anyhow!(
                        "pass --outlayer-account, --wasm-url and --wasm-hash together"
                    ))
                }
            };
            let r = ensure_ok(
                chain
                    .call(
                        "set_sealer",
                        json!({"pk": [px, py], "executor_pk": epk, "outlayer": outlayer}),
                    )
                    .await?,
            )?;
            println!("sealer registered: {}", describe(&r));
        }
        SealerCmd::Clear {
            pair,
            epoch,
            lookback,
        } => {
            let chain = cli.chain().await?;
            let secrets = sealer_secrets(&sec)?;
            let cleared = match epoch {
                Some(e) => vec![ops::clear_epoch(&chain, &secrets, *pair, *e).await?],
                None => ops::clear_pending(&chain, &secrets, *pair, *lookback).await?,
            };
            if cleared.is_empty() {
                println!("nothing to clear");
            }
            for c in cleared {
                println!(
                    "epoch {} cleared: {} sealed swaps, totals {} / {} | {}",
                    c.epoch, c.entries, c.delta_1, c.delta_2, c.summary
                );
            }
        }
        SealerCmd::Watch {
            pair,
            interval_secs,
            lookback,
        } => {
            let chain = cli.chain().await?;
            let secrets = sealer_secrets(&sec)?;
            println!("sealer watching pair {pair} every {interval_secs}s; ctrl-c to stop");
            loop {
                match ops::clear_pending(&chain, &secrets, *pair, *lookback).await {
                    Ok(cleared) => {
                        for c in cleared {
                            println!(
                                "epoch {} cleared: {} sealed swaps, totals {} / {} | {}",
                                c.epoch, c.entries, c.delta_1, c.delta_2, c.summary
                            );
                        }
                    }
                    Err(e) => eprintln!("clear failed: {e:#}"),
                }
                tokio::time::sleep(std::time::Duration::from_secs(*interval_secs)).await;
            }
        }
    }
    Ok(())
}
