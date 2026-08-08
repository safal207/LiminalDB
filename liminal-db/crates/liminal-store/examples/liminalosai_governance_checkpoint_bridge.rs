use std::env;
use std::io::{self, Read};
use std::path::PathBuf;

use liminal_store::{
    append_governance_checkpoint, CheckpointSigner, GovernanceTransitionEnvelope,
    GovernanceTransitionEnvelopeBody,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = env::args()
        .nth(1)
        .map(PathBuf::from)
        .ok_or("usage: liminalosai_governance_checkpoint_bridge <ledger-root>")?;
    let seed_hex = env::var("LIMINALDB_TEST_SIGNING_SEED_HEX")
        .map_err(|_| "LIMINALDB_TEST_SIGNING_SEED_HEX is required for this conformance helper")?;
    let issued_at_ms: u64 = env::var("LIMINALDB_CHECKPOINT_ISSUED_AT_MS")
        .map_err(|_| "LIMINALDB_CHECKPOINT_ISSUED_AT_MS is required")?
        .parse()?;

    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;
    let body: GovernanceTransitionEnvelopeBody = serde_json::from_str(&input)?;
    let envelope = GovernanceTransitionEnvelope::build(body)?;
    let signer = CheckpointSigner::from_seed_hex(
        "liminalosai-governance-bridge",
        "conformance-key-v0.1",
        &seed_hex,
    )?;
    let bundle = append_governance_checkpoint(root, envelope, &signer, issued_at_ms)?;
    println!("{}", serde_json::to_string(&bundle)?);
    Ok(())
}
