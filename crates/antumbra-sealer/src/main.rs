//! WASI entry point: reads one JSON request from stdin, writes one JSON
//! response to stdout. The secret comes from `SEALER_SEED` (on OutLayer: a
//! PROTECTED_ secret generated inside the TEE and injected as this variable).
use antumbra_core::sealing::SealerKey;
use std::io::{Read, Write};

fn main() {
    let mut input = String::new();
    std::io::stdin().read_to_string(&mut input).expect("stdin");
    let seed = std::env::var("SEALER_SEED")
        .or_else(|_| std::env::var("PROTECTED_SEALER_SEED"))
        .unwrap_or_default();
    let output = if seed.is_empty() {
        serde_json::json!({"ok": false, "error": "no sealer secret in the environment"})
    } else {
        let key = SealerKey::from_seed(seed.as_bytes());
        match serde_json::from_str(&input)
            .map_err(anyhow::Error::from)
            .and_then(|req| antumbra_sealer::handle(&key, &req))
        {
            Ok(v) => v,
            Err(e) => serde_json::json!({"ok": false, "error": e.to_string()}),
        }
    };
    let mut stdout = std::io::stdout();
    stdout
        .write_all(output.to_string().as_bytes())
        .expect("stdout");
    stdout.flush().expect("flush");
}
