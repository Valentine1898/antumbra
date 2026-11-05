//! Proving keys, regenerated from the deterministic setups. Natively they are
//! cached on disk; in the browser the caller stores the bytes itself.

use antumbra_circuits::{keys, Bls12_381, ProvingKey};
use anyhow::Result;
use ark_serialize::{CanonicalDeserialize, CanonicalSerialize};

pub struct ProvingKeys {
    pub joinsplit: ProvingKey<Bls12_381>,
    pub swap: ProvingKey<Bls12_381>,
    pub swapclaim: ProvingKey<Bls12_381>,
}

pub const CIRCUITS: [&str; 3] = ["joinsplit", "swap", "swapclaim"];

/// Runs one circuit's setup and returns the serialised proving key.
pub fn generate_bytes(name: &str) -> Result<Vec<u8>> {
    let (pk, _) = match name {
        "joinsplit" => keys::joinsplit_setup(),
        "swap" => keys::swap_setup(),
        "swapclaim" => keys::swapclaim_setup(),
        other => return Err(anyhow::anyhow!("unknown circuit {other}")),
    };
    let mut v = Vec::new();
    pk.serialize_uncompressed(&mut v)?;
    Ok(v)
}

fn parse(bytes: &[u8]) -> Result<ProvingKey<Bls12_381>> {
    Ok(ProvingKey::<Bls12_381>::deserialize_uncompressed_unchecked(
        bytes,
    )?)
}

impl ProvingKeys {
    pub fn from_bytes(joinsplit: &[u8], swap: &[u8], swapclaim: &[u8]) -> Result<Self> {
        Ok(ProvingKeys {
            joinsplit: parse(joinsplit)?,
            swap: parse(swap)?,
            swapclaim: parse(swapclaim)?,
        })
    }

    pub fn generate() -> Self {
        ProvingKeys {
            joinsplit: keys::joinsplit_setup().0,
            swap: keys::swap_setup().0,
            swapclaim: keys::swapclaim_setup().0,
        }
    }

    /// Loads from `<dir>/<circuit>.pk.bin`, generating and caching what is missing.
    #[cfg(feature = "native")]
    pub fn load_or_generate(dir: &std::path::Path) -> Result<Self> {
        std::fs::create_dir_all(dir)?;
        let mut loaded = Vec::new();
        for name in CIRCUITS {
            let path = dir.join(format!("{name}.pk.bin"));
            let bytes = match std::fs::read(&path) {
                Ok(b) if parse(&b).is_ok() => b,
                _ => {
                    let b = generate_bytes(name)?;
                    std::fs::write(&path, &b)?;
                    b
                }
            };
            loaded.push(bytes);
        }
        Self::from_bytes(&loaded[0], &loaded[1], &loaded[2])
    }
}
