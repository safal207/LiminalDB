use std::path::Path;

use liminal_store::{
    recover_interrupted_snapshot_replace, AuthorityState, CausalValidityState,
    ContinuityPosture, ExecutionState, ResponseIntegrityState, TrustworthyTransitionLedger,
};
use serde_json::json;

const TRANSITION_ID: &str = "airbnb-garden-29702510829";
const DESCENDANT_REF: &str =
    "sha256:200e0076823c241cdb05db79fce50ad51bef01f4f0456e8fa2ebd93ae2809619";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = std::env::args().nth(1).ok_or("missing ledger root")?;
    let snapshot_path = {
        let ledger = TrustworthyTransitionLedger::open(Path::new(&root))?;
        ledger.snapshot_path().to_path_buf()
    };

    recover_interrupted_snapshot_replace(&snapshot_path)?;
    let ledger = TrustworthyTransitionLedger::open(Path::new(&root))?;
    let projection = ledger
        .projection(TRANSITION_ID)
        .ok_or("missing verified-negative projection")?;
    let dimensions = projection.dimensions.as_ref().ok_or("missing dimensions")?;

    if ledger.event_count() != 6
        || projection.continuity_snapshot_ref.as_deref() != Some(DESCENDANT_REF)
        || projection.side_effect_committed
        || dimensions.authority != AuthorityState::Valid
        || dimensions.execution != ExecutionState::ObservedExecuted
        || dimensions.response_integrity != ResponseIntegrityState::Verified
        || dimensions.causal_validity != CausalValidityState::NotEvaluated
        || dimensions.continuity_posture != ContinuityPosture::ReportOnly
    {
        return Err("recovered projection mismatch".into());
    }

    let parent = snapshot_path.parent().ok_or("snapshot has no parent")?;
    let name = snapshot_path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or("invalid snapshot name")?;
    let temporary = parent.join(format!(".{name}.tmp"));
    let rollback = parent.join(format!(".{name}.rollback"));
    if temporary.exists() || rollback.exists() {
        return Err("recovery left temporary snapshot state".into());
    }

    println!(
        "VERIFIED_NEGATIVE_CRASH_RECEIPT={}",
        serde_json::to_string(&json!({
            "schema_version": "liminaldb-verified-negative-crash-receipt-v0.1",
            "transition_id": TRANSITION_ID,
            "event_count": ledger.event_count(),
            "continuity_ref": projection.continuity_snapshot_ref,
            "recovered": true,
            "recovery_artifacts_removed": true,
            "partial_projection_observed": false,
            "dimensions": {
                "authority": "VALID",
                "execution": "OBSERVED_EXECUTED",
                "response_integrity": "VERIFIED",
                "causal_validity": "NOT_EVALUATED",
                "continuity_posture": "REPORT_ONLY"
            },
            "memory": {
                "durable_memory_accepted": false,
                "production_write": false
            },
            "authority": {
                "external_submission": false,
                "deployment": false,
                "merge": false
            }
        }))?
    );
    Ok(())
}
