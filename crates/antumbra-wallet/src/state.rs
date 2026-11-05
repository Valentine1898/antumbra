//! Wallet file: keys, known notes and swaps, and a cache of every leaf
//! commitment (needed to build Merkle paths). Plain JSON, one file.

use antumbra_core::{
    crypto::{EncryptionKey, PublicKey},
    encoding::{fr_from_hex, fr_to_hex},
    note::{Note, SpendKey, SwapRecord},
    Fr,
};
use anyhow::{anyhow, Context, Result};
use ark_ff::UniformRand;
use ark_std::rand::{rngs::StdRng, SeedableRng};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fs, path::Path};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NoteEntry {
    pub asset_id: u32,
    pub amount: String,
    pub rseed: String,
    pub position: u64,
    pub spent: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SwapEntry {
    pub epoch: u64,
    pub pair_id: u32,
    pub direction: u8,
    pub delta: String,
    pub rseed: String,
    pub position: u64,
    pub claimed: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WalletFile {
    pub spend_key: String,
    pub enc_secret: String,
    #[serde(default)]
    pub notes: Vec<NoteEntry>,
    #[serde(default)]
    pub swaps: Vec<SwapEntry>,
    /// Every leaf commitment of the pool, by position.
    #[serde(default)]
    pub leaves: Vec<String>,
}

/// A shielded address: owner key and delivery key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Address {
    pub pk: Fr,
    pub enc_pk: [u8; 32],
}

impl Address {
    pub fn encode(&self) -> String {
        format!(
            "antumbra:{}:{}",
            fr_to_hex(&self.pk),
            hex::encode(self.enc_pk)
        )
    }

    pub fn parse(s: &str) -> Result<Self> {
        let parts: Vec<&str> = s.split(':').collect();
        if parts.len() != 3 || parts[0] != "antumbra" {
            return Err(anyhow!(
                "address must look like antumbra:<pk hex>:<enc hex>"
            ));
        }
        let pk = fr_from_hex(parts[1]).ok_or_else(|| anyhow!("bad owner key in address"))?;
        let enc: [u8; 32] = hex::decode(parts[2])?
            .try_into()
            .map_err(|_| anyhow!("bad delivery key in address"))?;
        Ok(Address { pk, enc_pk: enc })
    }

    pub fn public_key(&self) -> PublicKey {
        PublicKey::from(self.enc_pk)
    }
}

impl WalletFile {
    pub fn generate() -> Self {
        let sk = Fr::rand(&mut StdRng::from_seed(rand::random()));
        let enc: [u8; 32] = rand::random();
        WalletFile {
            spend_key: fr_to_hex(&sk),
            enc_secret: hex::encode(enc),
            notes: vec![],
            swaps: vec![],
            leaves: vec![],
        }
    }

    pub fn load(path: &Path) -> Result<Self> {
        let text = fs::read_to_string(path)
            .with_context(|| format!("reading wallet {}", path.display()))?;
        Ok(serde_json::from_str(&text)?)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(dir) = path.parent() {
            if !dir.as_os_str().is_empty() {
                fs::create_dir_all(dir)?;
            }
        }
        fs::write(path, serde_json::to_string_pretty(self)?)?;
        Ok(())
    }

    pub fn spend_key(&self) -> SpendKey {
        SpendKey(fr_from_hex(&self.spend_key).expect("wallet spend key"))
    }

    pub fn enc_key(&self) -> EncryptionKey {
        let bytes: [u8; 32] = hex::decode(&self.enc_secret)
            .expect("hex")
            .try_into()
            .expect("32 bytes");
        EncryptionKey::from_bytes(bytes)
    }

    pub fn address(&self) -> Address {
        Address {
            pk: self.spend_key().pk(),
            enc_pk: *self.enc_key().public.as_bytes(),
        }
    }

    pub fn leaves_fr(&self) -> Vec<Fr> {
        self.leaves
            .iter()
            .map(|h| fr_from_hex(h).expect("cached leaf"))
            .collect()
    }

    pub fn note(&self, e: &NoteEntry) -> Note {
        Note {
            asset_id: e.asset_id,
            amount: e.amount.parse().expect("amount"),
            pk: self.spend_key().pk(),
            rseed: fr_from_hex(&e.rseed).expect("rseed"),
        }
    }

    pub fn swap(&self, e: &SwapEntry) -> SwapRecord {
        SwapRecord {
            epoch: e.epoch,
            pair_id: e.pair_id,
            direction: e.direction,
            delta: e.delta.parse().expect("delta"),
            claim_pk: self.spend_key().pk(),
            rseed: fr_from_hex(&e.rseed).expect("rseed"),
        }
    }

    /// Unspent balance per asset id.
    pub fn balances(&self) -> BTreeMap<u32, u128> {
        let mut out = BTreeMap::new();
        for n in self.notes.iter().filter(|n| !n.spent) {
            *out.entry(n.asset_id).or_insert(0) += n.amount.parse::<u128>().expect("amount");
        }
        out
    }

    /// Picks at most two unspent notes of `asset_id` covering `amount`, largest first.
    pub fn select_inputs(&self, asset_id: u32, amount: u128) -> Result<Vec<usize>> {
        let mut candidates: Vec<(usize, u128)> = self
            .notes
            .iter()
            .enumerate()
            .filter(|(_, n)| !n.spent && n.asset_id == asset_id)
            .map(|(i, n)| (i, n.amount.parse::<u128>().expect("amount")))
            .collect();
        candidates.sort_by(|a, b| b.1.cmp(&a.1));
        let total: u128 = candidates.iter().map(|c| c.1).sum();
        if total < amount {
            return Err(anyhow!(
                "insufficient shielded balance for asset {asset_id}: have {total}, need {amount}"
            ));
        }
        if candidates[0].1 >= amount {
            return Ok(vec![candidates[0].0]);
        }
        if candidates.len() >= 2 && candidates[0].1 + candidates[1].1 >= amount {
            return Ok(vec![candidates[0].0, candidates[1].0]);
        }
        Err(anyhow!(
            "amount is spread over more than two notes; run `transfer` to yourself to merge first"
        ))
    }
}
