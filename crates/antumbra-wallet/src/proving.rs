//! Proving keys, regenerated from the deterministic setups and cached on disk.

use antumbra_circuits::{keys, Bls12_381, ProvingKey};
use anyhow::Result;
use ark_serialize::{CanonicalDeserialize, CanonicalSerialize};
use std::{fs, path::Path};

pub struct ProvingKeys {
    pub joinsplit: ProvingKey<Bls12_381>,
    pub swap: ProvingKey<Bls12_381>,
    pub swapclaim: ProvingKey<Bls12_381>,
}

fn cached(dir: &Path, name: &str, generate: fn() -> keys::Keys) -> Result<ProvingKey<Bls12_381>> {
    let path = dir.join(format!("{name}.pk.bin"));
    if let Ok(bytes) = fs::read(&path) {
        if let Ok(pk) = ProvingKey::<Bls12_381>::deserialize_uncompressed_unchecked(&bytes[..]) {
            return Ok(pk);
        }
    }
    let (pk, _) = generate();
    let mut v = Vec::new();
    pk.serialize_uncompressed(&mut v)?;
    fs::write(&path, v)?;
    Ok(pk)
}

impl ProvingKeys {
    pub fn load_or_generate(dir: &Path) -> Result<Self> {
        fs::create_dir_all(dir)?;
        Ok(ProvingKeys {
            joinsplit: cached(dir, "joinsplit", keys::joinsplit_setup)?,
            swap: cached(dir, "swap", keys::swap_setup)?,
            swapclaim: cached(dir, "swapclaim", keys::swapclaim_setup)?,
        })
    }
}
