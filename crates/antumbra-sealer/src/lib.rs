//! Sealer logic shared by the WASI program (OutLayer), the executor daemon
//! and the tests: decrypt every entry of a batch, check its commitment, sum
//! the amounts per direction.

use antumbra_core::{
    encoding::{fr_from_bytes, fr_to_bytes},
    sealing::{self, entries_hash, SealedEntry, SealerKey},
};
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// JSON form of a sealed entry (hex fields), as the contract's view returns it.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EntryJson {
    pub direction: u8,
    pub delta_cm: String,
    pub epk: [String; 2],
    pub ct: [String; 2],
}

fn h32(s: &str) -> Result<[u8; 32]> {
    hex::decode(s)?
        .try_into()
        .map_err(|_| anyhow!("expected 32 bytes"))
}

impl EntryJson {
    pub fn to_entry(&self) -> Result<SealedEntry> {
        Ok(SealedEntry {
            direction: self.direction,
            delta_cm: h32(&self.delta_cm)?,
            epk_x: h32(&self.epk[0])?,
            epk_y: h32(&self.epk[1])?,
            ct0: h32(&self.ct[0])?,
            ct1: h32(&self.ct[1])?,
        })
    }
    pub fn from_entry(e: &SealedEntry) -> Self {
        EntryJson {
            direction: e.direction,
            delta_cm: hex::encode(e.delta_cm),
            epk: [hex::encode(e.epk_x), hex::encode(e.epk_y)],
            ct: [hex::encode(e.ct0), hex::encode(e.ct1)],
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Totals {
    pub epoch: u64,
    pub pair_id: u32,
    pub entries_hash: [u8; 32],
    pub delta_1: u128,
    pub delta_2: u128,
    /// Indices whose ciphertext did not open to its commitment. Always empty
    /// for entries whose proof verified; the contract refuses a batch with any.
    pub invalid: Vec<usize>,
}

/// Opens every entry and sums the amounts per direction.
pub fn clear(key: &SealerKey, epoch: u64, pair_id: u32, entries: &[SealedEntry]) -> Totals {
    let mut t = Totals {
        epoch,
        pair_id,
        entries_hash: entries_hash(entries),
        delta_1: 0,
        delta_2: 0,
        invalid: vec![],
    };
    for (i, e) in entries.iter().enumerate() {
        let opened = (|| {
            let epk = sealing::point_from_xy(fr_from_bytes(&e.epk_x)?, fr_from_bytes(&e.epk_y)?)?;
            let ct = [fr_from_bytes(&e.ct0)?, fr_from_bytes(&e.ct1)?];
            sealing::unseal(key, &epk, &ct, fr_from_bytes(&e.delta_cm)?)
        })();
        match opened {
            Some((delta, _)) if e.direction == 1 => t.delta_1 += delta,
            Some((delta, _)) if e.direction == 2 => t.delta_2 += delta,
            _ => t.invalid.push(i),
        }
    }
    t
}

/// The request/response protocol of the WASI program (stdin JSON in, stdout JSON out).
pub fn handle(key: &SealerKey, request: &Value) -> Result<Value> {
    match request["op"].as_str().unwrap_or("") {
        "pubkey" => {
            let (x, y) = key.pk_xy();
            Ok(
                json!({"ok": true, "pk": [hex::encode(fr_to_bytes(&x)), hex::encode(fr_to_bytes(&y))]}),
            )
        }
        "clear" => {
            let epoch = request["epoch"].as_u64().ok_or_else(|| anyhow!("epoch"))?;
            let pair_id = request["pair_id"]
                .as_u64()
                .ok_or_else(|| anyhow!("pair_id"))? as u32;
            let entries_json: Vec<EntryJson> = serde_json::from_value(request["entries"].clone())?;
            let entries: Vec<SealedEntry> = entries_json
                .iter()
                .map(EntryJson::to_entry)
                .collect::<Result<_>>()?;
            let t = clear(key, epoch, pair_id, &entries);
            Ok(json!({
                "ok": true, "epoch": t.epoch, "pair_id": t.pair_id, "entries_hash": hex::encode(t.entries_hash),
                "delta_1": t.delta_1.to_string(), "delta_2": t.delta_2.to_string(), "invalid": t.invalid,
            }))
        }
        other => Err(anyhow!("unknown op {other:?}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use antumbra_core::{sealing::scalar_from_seed, Fr};

    #[test]
    fn clears_a_batch() {
        let key = SealerKey::from_seed(b"k");
        let mk = |delta: u128, direction: u8, seed: u8| {
            let s = sealing::seal(
                &key.pk,
                delta,
                Fr::from(seed as u64),
                scalar_from_seed(&[seed; 32]),
            );
            let (ex, ey) = sealing::point_xy(&s.epk);
            SealedEntry {
                direction,
                delta_cm: fr_to_bytes(&s.delta_cm),
                epk_x: fr_to_bytes(&ex),
                epk_y: fr_to_bytes(&ey),
                ct0: fr_to_bytes(&s.ct[0]),
                ct1: fr_to_bytes(&s.ct[1]),
            }
        };
        let mut entries = vec![mk(100, 1, 1), mk(250, 1, 2), mk(40, 2, 3)];
        let t = clear(&key, 9, 1, &entries);
        assert_eq!((t.delta_1, t.delta_2, t.invalid.len()), (350, 40, 0));
        entries[1].delta_cm = [9u8; 32];
        let t = clear(&key, 9, 1, &entries);
        assert_eq!((t.delta_1, t.invalid), (100, vec![1]));
        let out = handle(&key, &json!({"op": "clear", "epoch": 9, "pair_id": 1, "entries": entries.iter().map(EntryJson::from_entry).collect::<Vec<_>>()})).unwrap();
        assert_eq!(out["delta_2"], "40");
    }
}
