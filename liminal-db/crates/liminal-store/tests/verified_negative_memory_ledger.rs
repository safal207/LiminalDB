use liminal_store::{
    AuthorityState, CausalValidityState, ContinuityPosture, ExecutionState,
    ResponseIntegrityState, TransitionDimensions, TransitionEventInput, TransitionLinks,
    TransitionRecordKind, TrustworthyTransitionLedger,
};
use serde::Deserialize;
use serde_json::{json, Value};
use tempfile::tempdir;

#[derive(Debug, Clone, Deserialize)]
struct DigestRecord {
    record_ref: String,
    payload_digest: String,
}

#[derive(Debug, Clone, Deserialize)]
struct ObservationRecord {
    record_ref: String,
    payload_digest: String,
    result_ref: String,
}

#[derive(Debug, Clone, Deserialize)]
struct Records {
    authorization: DigestRecord,
    observation: ObservationRecord,
    response_integrity: DigestRecord,
    causal_audit: DigestRecord,
    continuity: DigestRecord,
}

#[derive(Debug, Clone, Deserialize)]
struct Judgment {
    verdict: String,
    verdict_scope: String,
    decision_status: String,
    result_class: String,
    cause_status: String,
    confidence: String,
    fabricated_claim_control: String,
    supported_claim_count: u64,
    durable_memory: bool,
}

#[derive(Debug, Clone, Deserialize)]
struct Authority {
    mode: String,
    ownership: bool,
    approval: bool,
    execution: bool,
    delivery: bool,
    external_submission: bool,
    deployment: bool,
    merge: bool,
    durable_memory_write: bool,
}

#[derive(Debug, Clone, Deserialize)]
struct RehearsalBoundary {
    ledger_root: String,
    production_write: bool,
    side_effect_committed: bool,
    delete_after_test: bool,
}

#[derive(Debug, Clone, Deserialize)]
struct Source {
    transition_run_id: u64,
    transition_artifact_id: u64,
    transition_artifact_sha256: String,
    downstream_run_id: u64,
    downstream_artifact_id: u64,
    downstream_artifact_sha256: String,
}

#[derive(Debug, Clone, Deserialize)]
struct Fixture {
    schema_version: String,
    transition_id: String,
    subject_id: String,
    captured_at_ms: u64,
    source: Source,
    records: Records,
    judgment: Judgment,
    authority: Authority,
    rehearsal_boundary: RehearsalBoundary,
}

fn fixture() -> Fixture {
    serde_json::from_str(include_str!(
        "fixtures/pythia_verified_negative_airbnb_v0.1.json"
    ))
    .expect("exact verified-negative fixture must decode")
}

fn is_sha256_ref(value: &str) -> bool {
    value.len() == 71
        && value.starts_with("sha256:")
        && value[7..].bytes().all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

fn validate_candidate(fixture: &Fixture) -> Result<(), String> {
    if fixture.schema_version != "liminaldb-verified-negative-ledger-replay-fixture-v0.1" {
        return Err("unsupported fixture schema".into());
    }
    if fixture.judgment.verdict != "ALLOW"
        || fixture.judgment.verdict_scope
            != "artifact_only_verified_negative_memory_candidate"
        || fixture.judgment.decision_status != "CONFIRMED"
        || fixture.judgment.result_class != "VERIFIED_NEGATIVE_OBSERVATION"
        || fixture.judgment.cause_status != "UNCONFIRMED"
        || fixture.judgment.confidence != "OBSERVED_ONCE"
        || fixture.judgment.fabricated_claim_control != "CONTRADICTED"
        || fixture.judgment.supported_claim_count != 8
        || fixture.judgment.durable_memory
    {
        return Err("judgment boundary mismatch".into());
    }
    if fixture.authority.mode != "audit_only"
        || fixture.authority.ownership
        || fixture.authority.approval
        || fixture.authority.execution
        || fixture.authority.delivery
        || fixture.authority.external_submission
        || fixture.authority.deployment
        || fixture.authority.merge
        || fixture.authority.durable_memory_write
    {
        return Err("authority escalation detected".into());
    }
    if fixture.rehearsal_boundary.ledger_root != "temporary_directory"
        || fixture.rehearsal_boundary.production_write
        || fixture.rehearsal_boundary.side_effect_committed
        || !fixture.rehearsal_boundary.delete_after_test
    {
        return Err("rehearsal boundary mismatch".into());
    }
    let refs = [
        &fixture.records.authorization.record_ref,
        &fixture.records.authorization.payload_digest,
        &fixture.records.observation.record_ref,
        &fixture.records.observation.payload_digest,
        &fixture.records.observation.result_ref,
        &fixture.records.response_integrity.record_ref,
        &fixture.records.response_integrity.payload_digest,
        &fixture.records.causal_audit.record_ref,
        &fixture.records.causal_audit.payload_digest,
        &fixture.records.continuity.record_ref,
        &fixture.records.continuity.payload_digest,
    ];
    if refs.iter().any(|value| !is_sha256_ref(value)) {
        return Err("invalid SHA-256 reference".into());
    }
    if fixture.source.transition_run_id != 29_702_510_829
        || fixture.source.transition_artifact_id != 8_446_921_466
        || fixture.source.downstream_run_id != 29_703_167_897
        || fixture.source.downstream_artifact_id != 8_447_067_600
        || fixture.source.transition_artifact_sha256
            != "4b33451c6d96d72705c18d88613555fdcb9be16af03425cb59539585ea9e11be"
        || fixture.source.downstream_artifact_sha256
            != "29279d724760af558b4d11c6ce1bbcb6c99e6aff238da35cc50e26e1ff3f9866"
    {
        return Err("source evidence mismatch".into());
    }
    Ok(())
}

fn dimensions(
    response_integrity: ResponseIntegrityState,
    causal_validity: CausalValidityState,
    continuity_posture: ContinuityPosture,
) -> TransitionDimensions {
    TransitionDimensions {
        authority: AuthorityState::Valid,
        execution: ExecutionState::ObservedExecuted,
        response_integrity,
        causal_validity,
        continuity_posture,
    }
}

fn input(
    fixture: &Fixture,
    kind: TransitionRecordKind,
    record: &DigestRecord,
    links: TransitionLinks,
    dimensions: Option<TransitionDimensions>,
    captured_offset: u64,
) -> TransitionEventInput {
    TransitionEventInput {
        transition_id: fixture.transition_id.clone(),
        subject_id: fixture.subject_id.clone(),
        kind,
        record_ref: record.record_ref.clone(),
        payload_digest: record.payload_digest.clone(),
        links,
        dimensions,
        side_effect_committed: Some(false),
        captured_at_ms: fixture.captured_at_ms + captured_offset,
    }
}

#[test]
fn verified_negative_candidate_survives_snapshot_and_wal_replay() -> Result<(), Box<dyn std::error::Error>> {
    let fixture = fixture();
    validate_candidate(&fixture)?;
    let root = tempdir()?;

    let (expected_projection, snapshot_digest, event_hashes) = {
        let mut ledger = TrustworthyTransitionLedger::open(root.path())?;
        let authorization = ledger.append(input(
            &fixture,
            TransitionRecordKind::Authorization,
            &fixture.records.authorization,
            TransitionLinks::default(),
            None,
            0,
        ))?;

        let observation_record = DigestRecord {
            record_ref: fixture.records.observation.record_ref.clone(),
            payload_digest: fixture.records.observation.payload_digest.clone(),
        };
        let observation = ledger.append(input(
            &fixture,
            TransitionRecordKind::Observation,
            &observation_record,
            TransitionLinks {
                authorization_ref: Some(authorization.body.record_ref.clone()),
                ..TransitionLinks::default()
            },
            Some(dimensions(
                ResponseIntegrityState::NotEvaluated,
                CausalValidityState::NotEvaluated,
                ContinuityPosture::ReportOnly,
            )),
            1,
        ))?;

        let response_integrity = ledger.append(input(
            &fixture,
            TransitionRecordKind::ResponseIntegrity,
            &fixture.records.response_integrity,
            TransitionLinks {
                authorization_ref: Some(authorization.body.record_ref.clone()),
                observation_refs: vec![observation.body.record_ref.clone()],
                ..TransitionLinks::default()
            },
            Some(dimensions(
                ResponseIntegrityState::Verified,
                CausalValidityState::NotEvaluated,
                ContinuityPosture::ReportOnly,
            )),
            2,
        ))?;

        let causal_audit = ledger.append(input(
            &fixture,
            TransitionRecordKind::CausalAudit,
            &fixture.records.causal_audit,
            TransitionLinks {
                authorization_ref: Some(authorization.body.record_ref.clone()),
                observation_refs: vec![observation.body.record_ref.clone()],
                response_integrity_ref: Some(response_integrity.body.record_ref.clone()),
                ..TransitionLinks::default()
            },
            Some(dimensions(
                ResponseIntegrityState::Verified,
                CausalValidityState::NotEvaluated,
                ContinuityPosture::ReportOnly,
            )),
            3,
        ))?;

        let continuity = ledger.append(input(
            &fixture,
            TransitionRecordKind::ContinuitySnapshot,
            &fixture.records.continuity,
            TransitionLinks {
                authorization_ref: Some(authorization.body.record_ref.clone()),
                observation_refs: vec![observation.body.record_ref.clone()],
                response_integrity_ref: Some(response_integrity.body.record_ref.clone()),
                causal_audit_ref: Some(causal_audit.body.record_ref.clone()),
                previous_continuity_ref: None,
            },
            Some(dimensions(
                ResponseIntegrityState::Verified,
                CausalValidityState::NotEvaluated,
                ContinuityPosture::ReportOnly,
            )),
            4,
        ))?;

        let snapshot = ledger.write_snapshot(fixture.captured_at_ms + 5)?;
        let projection = ledger
            .projection(&fixture.transition_id)
            .cloned()
            .expect("projection must exist");

        assert_eq!(ledger.event_count(), 5);
        assert_eq!(projection.authorization_ref.as_deref(), Some(fixture.records.authorization.record_ref.as_str()));
        assert_eq!(projection.observation_refs, vec![fixture.records.observation.record_ref.clone()]);
        assert_eq!(projection.response_integrity_ref.as_deref(), Some(fixture.records.response_integrity.record_ref.as_str()));
        assert_eq!(projection.causal_audit_ref.as_deref(), Some(fixture.records.causal_audit.record_ref.as_str()));
        assert_eq!(projection.continuity_snapshot_ref.as_deref(), Some(fixture.records.continuity.record_ref.as_str()));
        assert!(!projection.side_effect_committed);
        let final_dimensions = projection.dimensions.as_ref().expect("final dimensions");
        assert_eq!(final_dimensions.authority, AuthorityState::Valid);
        assert_eq!(final_dimensions.execution, ExecutionState::ObservedExecuted);
        assert_eq!(final_dimensions.response_integrity, ResponseIntegrityState::Verified);
        assert_eq!(final_dimensions.causal_validity, CausalValidityState::NotEvaluated);
        assert_eq!(final_dimensions.continuity_posture, ContinuityPosture::ReportOnly);

        (
            projection,
            snapshot.snapshot_digest,
            vec![
                authorization.event_hash,
                observation.event_hash,
                response_integrity.event_hash,
                causal_audit.event_hash,
                continuity.event_hash,
            ],
        )
    };

    let reopened = TrustworthyTransitionLedger::open(root.path())?;
    let recovered = reopened
        .projection(&fixture.transition_id)
        .expect("projection must recover after restart");
    assert_eq!(reopened.event_count(), 5);
    assert_eq!(recovered, &expected_projection);

    let receipt = json!({
        "schema_version": "liminaldb-verified-negative-ledger-replay-receipt-v0.1",
        "transition_id": fixture.transition_id,
        "subject_id": fixture.subject_id,
        "event_count": reopened.event_count(),
        "snapshot_digest": snapshot_digest,
        "event_hashes": event_hashes,
        "replay_equal": true,
        "dimensions": {
            "authority": "VALID",
            "execution": "OBSERVED_EXECUTED",
            "response_integrity": "VERIFIED",
            "causal_validity": "NOT_EVALUATED",
            "continuity_posture": "REPORT_ONLY"
        },
        "memory": {
            "result_class": fixture.judgment.result_class,
            "durable_memory_accepted": false,
            "storage_rehearsal": "EPHEMERAL_WAL_AND_SNAPSHOT",
            "production_write": false
        },
        "authority": {
            "external_submission": false,
            "deployment": false,
            "merge": false
        }
    });
    println!(
        "VERIFIED_NEGATIVE_LEDGER_RECEIPT={}",
        serde_json::to_string(&receipt)?
    );
    Ok(())
}

#[test]
fn artifact_only_candidate_rejects_durable_or_causal_escalation() {
    let source = include_str!("fixtures/pythia_verified_negative_airbnb_v0.1.json");

    let mut durable: Value = serde_json::from_str(source).expect("fixture JSON");
    durable["judgment"]["durable_memory"] = Value::Bool(true);
    let durable: Fixture = serde_json::from_value(durable).expect("typed fixture");
    assert!(validate_candidate(&durable).is_err());

    let mut causal: Value = serde_json::from_str(source).expect("fixture JSON");
    causal["judgment"]["cause_status"] = Value::String("CONFIRMED".into());
    let causal: Fixture = serde_json::from_value(causal).expect("typed fixture");
    assert!(validate_candidate(&causal).is_err());

    let mut external: Value = serde_json::from_str(source).expect("fixture JSON");
    external["authority"]["external_submission"] = Value::Bool(true);
    let external: Fixture = serde_json::from_value(external).expect("typed fixture");
    assert!(validate_candidate(&external).is_err());
}
