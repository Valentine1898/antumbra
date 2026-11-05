//! Routing a cleared batch through NEAR Intents (1Click).
//!
//! A batch nets the two directions of a pair against each other; only the
//! imbalance needs outside liquidity. This module prices that imbalance on
//! NEAR Intents and derives the clearing price every swapper in the batch
//! gets. The quote shape is the one a NEAR-resident pool uses on mainnet:
//! balances inside the Intents contract on both sides (`depositType` and
//! `recipientType` `INTENTS`), executed later as a signed intent by the
//! pool's registered key. Same-chain quotes with an on-chain deposit are not
//! offered for NEAR pairs, and confidential quotes need a 1Click JWT
//! (`ONECLICK_JWT`), in which case `confidentiality: basic` is requested.
//!
//! There is no testnet 1Click, so against the testnet pool this is a dry run:
//! real mainnet prices, nothing moves.

use anyhow::{anyhow, Context, Result};
use serde::Deserialize;
use serde_json::{json, Value};

pub const ONECLICK_API: &str = "https://1click.chaindefuser.com/v0";

#[derive(Debug, Clone, Deserialize)]
pub struct OneClickToken {
    #[serde(rename = "assetId")]
    pub asset_id: String,
    pub symbol: String,
    pub blockchain: String,
    pub decimals: u32,
    #[serde(default)]
    pub price: Option<f64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Quote {
    #[serde(rename = "amountIn")]
    pub amount_in: String,
    #[serde(rename = "amountOut")]
    pub amount_out: String,
    #[serde(rename = "amountOutFormatted")]
    pub amount_out_formatted: String,
    #[serde(rename = "minAmountOut")]
    pub min_amount_out: String,
    #[serde(rename = "timeEstimate")]
    pub time_estimate: u64,
    #[serde(rename = "amountInUsd", default)]
    pub amount_in_usd: Option<String>,
    #[serde(rename = "amountOutUsd", default)]
    pub amount_out_usd: Option<String>,
    #[serde(rename = "depositAddress", default)]
    pub deposit_address: Option<String>,
}

pub struct OneClick {
    http: reqwest::Client,
    jwt: Option<String>,
}

impl Default for OneClick {
    fn default() -> Self {
        Self::new()
    }
}

impl OneClick {
    /// Reads `ONECLICK_JWT` for the authenticated fee schedule and confidential quotes.
    pub fn new() -> Self {
        let http = reqwest::Client::builder()
            .user_agent("antumbra-router/0.1 (+https://github.com/Valentine1898/antumbra)")
            .build()
            .expect("http client");
        OneClick {
            http,
            jwt: std::env::var("ONECLICK_JWT").ok().filter(|s| !s.is_empty()),
        }
    }

    pub fn confidential(&self) -> bool {
        self.jwt.is_some()
    }

    pub async fn tokens(&self) -> Result<Vec<OneClickToken>> {
        let r = self
            .http
            .get(format!("{ONECLICK_API}/tokens"))
            .send()
            .await
            .context("1Click token list")?;
        Ok(r.error_for_status()?.json().await?)
    }

    /// The NEAR-side 1Click asset for one of the pool's tokens, matched by
    /// symbol (`USDC.e` on testnet stands in for `USDC`). Test tokens without
    /// a known symbol have no route.
    pub fn asset_for<'a>(tokens: &'a [OneClickToken], symbol: &str) -> Option<&'a OneClickToken> {
        let want = symbol.trim_end_matches(".e").to_ascii_uppercase();
        tokens
            .iter()
            .find(|t| t.blockchain == "near" && t.symbol.to_ascii_uppercase() == want)
    }

    /// Quotes `amount` (base units of `origin`) for `dest`, both as Intents
    /// balances of `account`. `dry` quotes carry no deposit address.
    pub async fn quote(
        &self,
        origin: &str,
        dest: &str,
        amount: u128,
        account: &str,
        dry: bool,
    ) -> Result<Quote> {
        let deadline = chrono_like_deadline();
        let mut body = json!({
            "dry": dry,
            "swapType": "EXACT_INPUT",
            "slippageTolerance": 100,
            "originAsset": origin,
            "depositType": "INTENTS",
            "destinationAsset": dest,
            "amount": amount.to_string(),
            "recipient": account,
            "recipientType": "INTENTS",
            "refundTo": account,
            "refundType": "INTENTS",
            "deadline": deadline,
        });
        if self.jwt.is_some() {
            body["confidentiality"] = json!("basic");
        }
        let mut req = self.http.post(format!("{ONECLICK_API}/quote")).json(&body);
        if let Some(jwt) = &self.jwt {
            req = req.bearer_auth(jwt);
        }
        let r = req.send().await.context("1Click quote")?;
        let status = r.status();
        let v: Value = r.json().await.context("1Click quote body")?;
        if !status.is_success() {
            return Err(anyhow!(
                "1Click refused the quote ({status}): {}",
                v.get("message").and_then(|m| m.as_str()).unwrap_or("?")
            ));
        }
        Ok(serde_json::from_value(
            v.get("quote")
                .cloned()
                .ok_or_else(|| anyhow!("no quote in reply"))?,
        )?)
    }
}

/// RFC 3339 deadline one hour from now, without pulling in a date crate.
fn chrono_like_deadline() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
        + 3600;
    // Days since the epoch to a civil date (Howard Hinnant's algorithm).
    let days = (secs / 86_400) as i64;
    let (h, m, s) = ((secs % 86_400) / 3600, (secs % 3600) / 60, secs % 60);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let mo = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if mo <= 2 { y + 1 } else { y };
    format!("{y:04}-{mo:02}-{d:02}T{h:02}:{m:02}:{s:02}.000Z")
}

/// One of the pool's tokens as the router sees it.
#[derive(Debug, Clone)]
pub struct Side {
    pub symbol: String,
    pub decimals: u32,
}

/// The imbalance of a batch, priced on NEAR Intents.
#[derive(Debug, Clone)]
pub struct Leg {
    pub sell: Side,
    pub buy: Side,
    pub amount: u128,
    pub quote: Quote,
}

/// How a batch would clear through NEAR Intents.
#[derive(Debug, Clone)]
pub struct RoutePlan {
    pub delta_1: u128,
    pub delta_2: u128,
    /// The net leg sent to Intents, if the batch does not net out exactly.
    pub leg: Option<Leg>,
    /// Asset 2 per asset 1, in token units.
    pub price: f64,
    /// What sellers of asset 1 receive in asset 2, and vice versa.
    pub lambda_1: u128,
    pub lambda_2: u128,
    pub confidential: bool,
    pub note: String,
}

fn units(base: u128, decimals: u32) -> f64 {
    base as f64 / 10f64.powi(decimals as i32)
}
fn base(units: f64, decimals: u32) -> u128 {
    (units * 10f64.powi(decimals as i32)).round().max(0.0) as u128
}

/// Nets a batch's two directions at the Intents price and quotes the
/// remainder. `account` is the pool account whose Intents balance would hold
/// the reserves on mainnet.
pub async fn plan(
    client: &OneClick,
    sides: (&Side, &Side),
    delta_1: u128,
    delta_2: u128,
    account: &str,
) -> Result<RoutePlan> {
    let tokens = client.tokens().await?;
    let a1 = OneClick::asset_for(&tokens, &sides.0.symbol)
        .ok_or_else(|| anyhow!("no NEAR Intents asset matches {}", sides.0.symbol))?;
    let a2 = OneClick::asset_for(&tokens, &sides.1.symbol)
        .ok_or_else(|| anyhow!("no NEAR Intents asset matches {}", sides.1.symbol))?;
    let (p1, p2) = (
        a1.price
            .ok_or_else(|| anyhow!("no price for {}", a1.symbol))?,
        a2.price
            .ok_or_else(|| anyhow!("no price for {}", a2.symbol))?,
    );
    // Reference price (asset 2 per asset 1) from the token list, used only to
    // decide which side is in surplus; the quote then sets the real price.
    let p0 = p1 / p2;
    let d1 = units(delta_1, sides.0.decimals);
    let d2 = units(delta_2, sides.1.decimals);
    let d2_in_1 = d2 / p0;
    let mut plan = RoutePlan {
        delta_1,
        delta_2,
        leg: None,
        price: p0,
        lambda_1: 0,
        lambda_2: 0,
        confidential: client.confidential(),
        note: String::new(),
    };
    if d1 == 0.0 && d2 == 0.0 {
        plan.note = "empty batch".into();
        return Ok(plan);
    }
    let price = if d1 >= d2_in_1 {
        let net = d1 - d2_in_1;
        let net_base = base(net, sides.0.decimals);
        if net_base == 0 {
            plan.note = "the two directions net out exactly; nothing leaves the pool".into();
            p0
        } else {
            let quote = client
                .quote(&a1.asset_id, &a2.asset_id, net_base, account, true)
                .await?;
            let out = units(quote.amount_out.parse::<u128>()?, a2.decimals);
            let p = out / net;
            plan.leg = Some(Leg {
                sell: sides.0.clone(),
                buy: sides.1.clone(),
                amount: net_base,
                quote,
            });
            p
        }
    } else {
        let net = d2 - d1 * p0;
        let net_base = base(net, sides.1.decimals);
        let quote = client
            .quote(&a2.asset_id, &a1.asset_id, net_base, account, true)
            .await?;
        let out = units(quote.amount_out.parse::<u128>()?, a1.decimals);
        let p = net / out;
        plan.leg = Some(Leg {
            sell: sides.1.clone(),
            buy: sides.0.clone(),
            amount: net_base,
            quote,
        });
        p
    };
    plan.price = price;
    plan.lambda_2 = base(d1 * price, sides.1.decimals);
    plan.lambda_1 = base(d2 / price, sides.0.decimals);
    if plan.note.is_empty() {
        plan.note = match &plan.leg {
            Some(leg) => format!(
                "the batch nets to {} {} sold on NEAR Intents for {} {} ({} s); everyone in the batch clears at {:.6} {} per {}",
                units(leg.amount, leg.sell.decimals),
                leg.sell.symbol,
                units(leg.quote.amount_out.parse().unwrap_or(0), leg.buy.decimals),
                leg.buy.symbol,
                leg.quote.time_estimate,
                price,
                sides.1.symbol,
                sides.0.symbol
            ),
            None => format!("clears at the reference price {:.6} {} per {}", price, sides.1.symbol, sides.0.symbol),
        };
    }
    Ok(plan)
}
