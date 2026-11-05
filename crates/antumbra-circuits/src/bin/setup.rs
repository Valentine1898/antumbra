//! Writes every verifying key in NEAR layout to keys/<circuit>.vk.near.
//! Deterministic: running it twice yields identical bytes.

use std::{fs, path::PathBuf};

fn main() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../keys");
    fs::create_dir_all(&dir).unwrap();
    for (name, bytes, hash) in antumbra_circuits::keys::all_vks_near() {
        let path = dir.join(format!("{name}.vk.near"));
        fs::write(&path, &bytes).unwrap();
        println!(
            "wrote {} ({} bytes, sha256 {})",
            path.display(),
            bytes.len(),
            hex::encode(hash)
        );
    }
}
