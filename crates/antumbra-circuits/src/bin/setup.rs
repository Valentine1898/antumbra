//! Writes the join-split verifying key in NEAR layout to keys/joinsplit.vk.near.
//! Deterministic: running it twice yields identical bytes.

use std::{fs, path::PathBuf};

fn main() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../keys");
    fs::create_dir_all(&dir).unwrap();
    let (bytes, hash) = antumbra_circuits::keys::joinsplit_vk_near();
    let path = dir.join("joinsplit.vk.near");
    fs::write(&path, &bytes).unwrap();
    println!(
        "wrote {} ({} bytes, sha256 {})",
        path.display(),
        bytes.len(),
        hex::encode(hash)
    );
}
