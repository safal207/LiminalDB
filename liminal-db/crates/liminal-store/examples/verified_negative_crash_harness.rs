use std::path::Path;

use liminal_store::{
    AuthorityState, CausalValidityState, ContinuityPosture, CrashSafeTransitionSnapshotExt,
    ExecutionState, ResponseIntegrityState, TransitionDimensions, TransitionEventInput,
    TransitionLinks, TransitionRecordKind, TrustworthyTransitionLedger,
};
use serde_json::json;

const TRANSITION_ID: &str = "airbnb-garden-29702510829";
const SUBJECT_ID: &str = "airbnb-listing-1418689551881927394";
const AUTHORIZATION_REF: &str =
    "sha256:f073104629d6298e10b0f17b2e5fa88c9abab84838335763944d648cf4c1b7cc";
const OBSERVATION_REF: &str =
    "sha256:9145606b8c541b6d5af62aab3c8d23fe05e31397b90482e37720cdec3ebf1d33";
const RESPONSE_REF: &str =
    "sha256:449b3c2310591c64a86b230100b3b3007a855d3fd9af9a824cf48d07daec3c78";
const CAUSAL_REF: &str =
    "sha256:81ca2b25d6c84eea89e8559091e2c6748977a483d88075c7627e5b6cb8c09262";
const CONTINUITY_REF: &str =
    "sha256:8ade7ca92261f1ee961440cd3696da82698ff933205d30dc813ecf6d87cbce74";
const DESCENDANT_REF: &str =
    "sha256:200e0076823c241cdb05db79fce50ad51bef01f4f0456e8fa2ebd93ae2809619";
const BASE_TIME: u64 = 1_784_493_600_000;

fn dims(integrity: ResponseIntegrityState) -> TransitionDimensions {
    TransitionDimensions {
        authority: AuthorityState::Valid,
        execution: ExecutionState::ObservedExecuted,
        response_integrity: integrity,
        causal_validity: CausalValidityState::NotEvaluated,
        continuity_posture: ContinuityPosture::ReportOnly,
    }
}

fn event(
    kind: TransitionRecordKind,
    record_ref: &str,
    payload_digest: &str,
    links: TransitionLinks,
    dimensions: Option<TransitionDimensions>,
    offset: u64,
) -> TransitionEventInput {
    TransitionEventInput {
        transition_id: TRANSITION_ID.to_owned(),
        subject_id: SUBJECT_ID.to_owned(),
        kind,
        record_ref: record_ref.to_owned(),
        payload_digest: payload_digest.to_owned(),
        links,
        dimensions,
        side_effect_committed: Some(false),
        captured_at_ms: BASE_TIME + offset,
    }
}

fn prepare(root: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let mut ledger = TrustworthyTransitionLedger::open(root)?;
    let authorization = ledger.append(event(
        TransitionRecordKind::Authorization,
        AUTHORIZATION_REF,
        "sha256:aeefb218ef0590ea40a5f6c40223a6da864abdcce5570598b9adb4771c7c28a8",
        TransitionLinks::default(),
        None,
        0,
    ))?;
    let observation = ledger.append(event(
        TransitionRecordKind::Observation,
        OBSERVATION_REF,
        "sha256:4939d2adf5cfd48d3cfa445b8175e4e129e8cbdb2d63e5caf6a1380abbeed29b",
        TransitionLinks {
            authorization_ref: Some(authorization.body.record_ref.clone()),
            ..TransitionLinks::default()
        },
        Some(dims(ResponseIntegrityState::NotEvaluated)),
        1,
    ))?;
    let response = ledger.append(event(
        TransitionRecordKind::ResponseIntegrity,
        RESPONSE_REF,
        RESPONSE_REF,
        TransitionLinks {
            authorization_ref: Some(authorization.body.record_ref.clone()),
            observation_refs: vec![observation.body.record_ref.clone()],
            ..TransitionLinks::default()
        },
        Some(dims(ResponseIntegrityState::Verified)),
        2,
    ))?;
    let causal = ledger.append(event(
        TransitionRecordKind::CausalAudit,
        CAUSAL_REF,
        "sha256:ce8f05643f51015c64364f5d13f0597289c1064bd1c3b4eb5c5f6c1824c5bb77",
        TransitionLinks {
            authorization_ref: Some(authorization.body.record_ref.clone()),
            observation_refs: vec![observation.body.record_ref.clone()],
            response_integrity_ref: Some(response.body.record_ref.clone()),
            ..TransitionLinks::default()
        },
        Some(dims(ResponseIntegrityState::Verified)),
        3,
    ))?;
    let continuity = ledger.append(event(
        TransitionRecordKind::ContinuitySnapshot,
        CONTINUITY_REF,
        "sha256:29e9a1d8f0ad2513d42c1aa2c9312e717cbb13e34067ec76ff110440ddff6de9",
        TransitionLinks {
            authorization_ref: Some(authorization.body.record_ref.clone()),
            observation_refs: vec![observation.body.record_ref.clone()],
            response_integrity_ref: Some(response.body.record_ref.clone()),
            causal_audit_ref: Some(causal.body.record_ref.clone()),
            previous_continuity_ref: None,
        },
        Some(dims(ResponseIntegrityState::Verified)),
        4,
    ))?;

    ledger.write_snapshot_crash_safe(BASE_TIME + 5)?;
    ledger.append(event(
        TransitionRecordKind::ContinuitySnapshot,
        DESCENDANT_REF,
        "sha256:521c236216e5f5ee606394e1594bf25e376ee5fbd1bb55d1a58f742a64450c22",
        TransitionLinks {
            authorization_ref: Some(authorization.body.record_ref),
            observation_refs: vec![observation.body.record_ref],
            response_integrity_ref: Some(response.body.record_ref),
            causal_audit_ref: Some(causal.body.record_ref),
            previous_continuity_ref: Some(continuity.body.record_ref),
        },
        Some(dims(ResponseIntegrityState::Verified)),
        6,
    ))?;
    assert_eq!(ledger.event_count(), 6);
    Ok(())
}

fn crash_snapshot(root: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let mut ledger = TrustworthyTransitionLedger::open(root)?;
    assert_eq!(ledger.event_count(), 6);
    ledger.write_snapshot_crash_safe(BASE_TIME + 7)?;
    eprintln!("snapshot unexpectedly completed without triggering failpoint");
    std::process::exit(87);
}

fn inspect(root: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let ledger = TrustworthyTransitionLedger::open(root)?;
    let projection = ledger
        .projection(TRANSITION_ID)
        .ok_or("missing verified-negative projection")?;
    if ledger.event_count() != 6
        || projection.continuity_snapshot_ref.as_deref() != Some(DESCENDANT_REF)
        || projection.side_effect_committed
    {
        return Err("recovered projection mismatch".into());
    }
    let dimensions = projection.dimensions.as_ref().ok_or("missing dimensions")?;
    if dimensions.authority != AuthorityState::Valid
        || dimensions.execution != ExecutionState::ObservedExecuted
        || dimensions.response_integrity != ResponseIntegrityState::Verified
        || dimensions.causal_validity != CausalValidityState::NotEvaluated
        || dimensions.continuity_posture != ContinuityPosture::ReportOnly
    {
        return Err("recovered dimensions mismatch".into());
    }

    println!(
        "VERIFIED_NEGATIVE_CRASH_RECEIPT={}",
        serde_json::to_string(&json!({
            "schema_version": "liminaldb-verified-negative-crash-receipt-v0.1",
            "transition_id": TRANSITION_ID,
            "event_count": ledger.event_count(),
            "continuity_ref": projection.continuity_snapshot_ref,
            "recovered": true,
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

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let command = args.get(1).ok_or("missing command")?;
    let root = args.get(2).ok_or("missing ledger root")?;
    match command.as_str() {
        "prepare" => prepare(Path::new(root)),
        "crash-snapshot" => crash_snapshot(Path::new(root)),
        "inspect" => inspect(Path::new(root)),
        other => Err(format!("unknown command: {other}").into()),
    }
}
