use liminal_store::{
    sha256_ref, verify_checkpoint_chain, verify_signed_checkpoint, AntiRollbackStatus,
    AuthorityState, CausalValidityState, CheckpointError, CheckpointLedgerExt, CheckpointSigner,
    ContinuityPosture, CrashSafeTransitionSnapshotExt, ExecutionState, ExternalAnchor,
    ResponseIntegrityState, TransitionDimensions, TransitionEventInput, TransitionLinks,
    TransitionRecordKind, TrustedKeyRegistry, TrustworthyTransitionLedger,
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
struct Records {
    authorization: DigestRecord,
    observation: DigestRecord,
    response_integrity: DigestRecord,
    causal_audit: DigestRecord,
    continuity: DigestRecord,
    continuity_descendant: DigestRecord,
}

#[derive(Debug, Clone, Deserialize)]
struct Source {
    transition_run_id: u64,
    transition_artifact_id: u64,
    transition_artifact_sha256: String,
    downstream_run_id: u64,
    downstream_artifact_id: u64,
    downstream_artifact_sha256: String,
    ledger_replay_run_id: u64,
    ledger_replay_artifact_id: u64,
    ledger_replay_artifact_sha256: String,
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
struct CheckpointConfig {
    storage_root_identity: String,
    signer_id: String,
    key_id: String,
    seed_hex: String,
    provider_profile: String,
    anchor_id: String,
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
    crash_simulation: bool,
    sudden_power_loss_claimed: bool,
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
    checkpoint: CheckpointConfig,
    authority: Authority,
    rehearsal_boundary: RehearsalBoundary,
}

fn fixture() -> Fixture {
    serde_json::from_str(include_str!(
        "fixtures/pythia_verified_negative_anchored_v0.1.json"
    ))
    .expect("anchored verified-negative fixture must decode")
}

fn validate_fixture(value: &Fixture) -> Result<(), String> {
    if value.schema_version != "liminaldb-verified-negative-anchored-fixture-v0.1" {
        return Err("unsupported fixture schema".into());
    }
    if value.transition_id != "airbnb-garden-29702510829"
        || value.subject_id != "airbnb-listing-1418689551881927394"
        || value.captured_at_ms != 1_784_493_600_000
    {
        return Err("top-level identity mismatch".into());
    }

    let source = &value.source;
    if source.transition_run_id != 29_702_510_829
        || source.transition_artifact_id != 8_446_921_466
        || source.transition_artifact_sha256
            != "4b33451c6d96d72705c18d88613555fdcb9be16af03425cb59539585ea9e11be"
        || source.downstream_run_id != 29_703_167_897
        || source.downstream_artifact_id != 8_447_067_600
        || source.downstream_artifact_sha256
            != "29279d724760af558b4d11c6ce1bbcb6c99e6aff238da35cc50e26e1ff3f9866"
        || source.ledger_replay_run_id != 29_703_523_625
        || source.ledger_replay_artifact_id != 8_447_178_293
        || source.ledger_replay_artifact_sha256
            != "4067c3e197a582affc83e4d0426780b11ccbfad05e884688053c2af0a0e8dde2"
    {
        return Err("source provenance mismatch".into());
    }

    let records = [
        (
            &value.records.authorization,
            "sha256:f073104629d6298e10b0f17b2e5fa88c9abab84838335763944d648cf4c1b7cc",
            "sha256:aeefb218ef0590ea40a5f6c40223a6da864abdcce5570598b9adb4771c7c28a8",
        ),
        (
            &value.records.observation,
            "sha256:9145606b8c541b6d5af62aab3c8d23fe05e31397b90482e37720cdec3ebf1d33",
            "sha256:4939d2adf5cfd48d3cfa445b8175e4e129e8cbdb2d63e5caf6a1380abbeed29b",
        ),
        (
            &value.records.response_integrity,
            "sha256:449b3c2310591c64a86b230100b3b3007a855d3fd9af9a824cf48d07daec3c78",
            "sha256:449b3c2310591c64a86b230100b3b3007a855d3fd9af9a824cf48d07daec3c78",
        ),
        (
            &value.records.causal_audit,
            "sha256:81ca2b25d6c84eea89e8559091e2c6748977a483d88075c7627e5b6cb8c09262",
            "sha256:ce8f05643f51015c64364f5d13f0597289c1064bd1c3b4eb5c5f6c1824c5bb77",
        ),
        (
            &value.records.continuity,
            "sha256:8ade7ca92261f1ee961440cd3696da82698ff933205d30dc813ecf6d87cbce74",
            "sha256:29e9a1d8f0ad2513d42c1aa2c9312e717cbb13e34067ec76ff110440ddff6de9",
        ),
        (
            &value.records.continuity_descendant,
            "sha256:200e0076823c241cdb05db79fce50ad51bef01f4f0456e8fa2ebd93ae2809619",
            "sha256:521c236216e5f5ee606394e1594bf25e376ee5fbd1bb55d1a58f742a64450c22",
        ),
    ];
    for (record, expected_ref, expected_digest) in records {
        if record.record_ref != expected_ref || record.payload_digest != expected_digest {
            return Err("record identity mismatch".into());
        }
    }

    let checkpoint = &value.checkpoint;
    if checkpoint.storage_root_identity
        != "sha256:22a2426f251fe30e9951f2c7ce9dcbe33d05db84c20236818c71246add7d85ef"
        || checkpoint.signer_id != "operator:liminal-test"
        || checkpoint.key_id != "airbnb-negative-key-1"
        || checkpoint.seed_hex != "1111111111111111111111111111111111111111111111111111111111111111"
        || checkpoint.provider_profile != "org.liminaldb.test-external-anchor.v0.1"
        || checkpoint.anchor_id != "airbnb-negative-anchor-29702510829"
    {
        return Err("checkpoint identity mismatch".into());
    }

    if value.judgment.verdict != "ALLOW"
        || value.judgment.verdict_scope != "artifact_only_verified_negative_memory_candidate"
        || value.judgment.decision_status != "CONFIRMED"
        || value.judgment.result_class != "VERIFIED_NEGATIVE_OBSERVATION"
        || value.judgment.cause_status != "UNCONFIRMED"
        || value.judgment.confidence != "OBSERVED_ONCE"
        || value.judgment.fabricated_claim_control != "CONTRADICTED"
        || value.judgment.supported_claim_count != 8
        || value.judgment.durable_memory
    {
        return Err("judgment boundary mismatch".into());
    }
    if value.authority.mode != "audit_only"
        || value.authority.ownership
        || value.authority.approval
        || value.authority.execution
        || value.authority.delivery
        || value.authority.external_submission
        || value.authority.deployment
        || value.authority.merge
        || value.authority.durable_memory_write
    {
        return Err("authority escalation detected".into());
    }
    if value.rehearsal_boundary.ledger_root != "temporary_directory"
        || value.rehearsal_boundary.production_write
        || value.rehearsal_boundary.side_effect_committed
        || !value.rehearsal_boundary.delete_after_test
        || !value.rehearsal_boundary.crash_simulation
        || value.rehearsal_boundary.sudden_power_loss_claimed
    {
        return Err("rehearsal boundary mismatch".into());
    }
    Ok(())
}

fn dimensions(
    integrity: ResponseIntegrityState,
    causal: CausalValidityState,
) -> TransitionDimensions {
    TransitionDimensions {
        authority: AuthorityState::Valid,
        execution: ExecutionState::ObservedExecuted,
        response_integrity: integrity,
        causal_validity: causal,
        continuity_posture: ContinuityPosture::ReportOnly,
    }
}

fn input(
    fixture: &Fixture,
    kind: TransitionRecordKind,
    record: &DigestRecord,
    links: TransitionLinks,
    dimensions: Option<TransitionDimensions>,
    offset: u64,
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
        captured_at_ms: fixture.captured_at_ms + offset,
    }
}

#[test]
fn verified_negative_checkpoint_is_signed_anchored_and_rollback_safe(
) -> Result<(), Box<dyn std::error::Error>> {
    let fixture = fixture();
    validate_fixture(&fixture).map_err(std::io::Error::other)?;
    let directory = tempdir()?;

    let signer = CheckpointSigner::from_seed_hex(
        &fixture.checkpoint.signer_id,
        &fixture.checkpoint.key_id,
        &fixture.checkpoint.seed_hex,
    )?;
    let registry = TrustedKeyRegistry::default().with_key(signer.trusted_key(0, None, None))?;

    let (checkpoint_one, checkpoint_two, anchor, expected_projection) = {
        let mut ledger = TrustworthyTransitionLedger::open(directory.path())?;
        let authorization = ledger.append(input(
            &fixture,
            TransitionRecordKind::Authorization,
            &fixture.records.authorization,
            TransitionLinks::default(),
            None,
            0,
        ))?;
        let observation = ledger.append(input(
            &fixture,
            TransitionRecordKind::Observation,
            &fixture.records.observation,
            TransitionLinks {
                authorization_ref: Some(authorization.body.record_ref.clone()),
                ..TransitionLinks::default()
            },
            Some(dimensions(
                ResponseIntegrityState::NotEvaluated,
                CausalValidityState::NotEvaluated,
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
            )),
            4,
        ))?;

        let snapshot_one = ledger.write_snapshot_crash_safe(fixture.captured_at_ms + 5)?;
        let material_one = ledger.checkpoint_material(
            fixture.checkpoint.storage_root_identity.clone(),
            &snapshot_one,
        )?;
        let checkpoint_one = signer.sign(material_one, fixture.captured_at_ms + 10, None, None)?;
        verify_signed_checkpoint(&checkpoint_one, &registry, fixture.captured_at_ms + 11)?;

        ledger.append(input(
            &fixture,
            TransitionRecordKind::ContinuitySnapshot,
            &fixture.records.continuity_descendant,
            TransitionLinks {
                authorization_ref: Some(authorization.body.record_ref),
                observation_refs: vec![observation.body.record_ref],
                response_integrity_ref: Some(response_integrity.body.record_ref),
                causal_audit_ref: Some(causal_audit.body.record_ref),
                previous_continuity_ref: Some(continuity.body.record_ref),
            },
            Some(dimensions(
                ResponseIntegrityState::Verified,
                CausalValidityState::NotEvaluated,
            )),
            12,
        ))?;

        let snapshot_two = ledger.write_snapshot_crash_safe(fixture.captured_at_ms + 13)?;
        let material_two = ledger.checkpoint_material(
            fixture.checkpoint.storage_root_identity.clone(),
            &snapshot_two,
        )?;
        let checkpoint_two = signer.sign(
            material_two.clone(),
            fixture.captured_at_ms + 20,
            None,
            Some(checkpoint_one.manifest_ref.clone()),
        )?;
        let anchor = ExternalAnchor {
            provider_profile: fixture.checkpoint.provider_profile.clone(),
            anchor_id: fixture.checkpoint.anchor_id.clone(),
            checkpoint_ref: checkpoint_two.manifest_ref.clone(),
            storage_root_identity: checkpoint_two.body.storage_root_identity.clone(),
            event_chain_head: checkpoint_two.body.event_chain_head.clone(),
            last_sequence: checkpoint_two.body.last_sequence,
            anchored_at_ms: fixture.captured_at_ms + 21,
        };

        let verified = verify_checkpoint_chain(
            &[checkpoint_one.clone(), checkpoint_two.clone()],
            &registry,
            Some(&anchor),
            fixture.captured_at_ms + 22,
        )?;
        assert_eq!(verified.status, AntiRollbackStatus::ExternalAnchorVerified);
        assert_eq!(verified.checkpoint_count, 2);
        assert_eq!(verified.latest_sequence, 6);

        let rollback = verify_checkpoint_chain(
            &[checkpoint_one.clone()],
            &registry,
            Some(&anchor),
            fixture.captured_at_ms + 22,
        )
        .expect_err("older checkpoint must be rejected against the anchor");
        assert_eq!(rollback, CheckpointError::ExternalAnchorRollback);

        let mut fork_material = material_two;
        fork_material.event_chain_head = sha256_ref(b"airbnb-negative-forked-head");
        let fork = signer.sign(
            fork_material,
            fixture.captured_at_ms + 20,
            None,
            Some(checkpoint_one.manifest_ref.clone()),
        )?;
        let fork_error = verify_checkpoint_chain(
            &[checkpoint_one.clone(), fork],
            &registry,
            Some(&anchor),
            fixture.captured_at_ms + 22,
        )
        .expect_err("same-sequence fork must be rejected");
        assert_eq!(fork_error, CheckpointError::ExternalAnchorFork);

        let mut tampered = checkpoint_two.clone();
        let replacement = if tampered.signature_hex.starts_with("00") {
            "01"
        } else {
            "00"
        };
        tampered.signature_hex.replace_range(0..2, replacement);
        let tamper_error =
            verify_signed_checkpoint(&tampered, &registry, fixture.captured_at_ms + 22)
                .expect_err("tampered signature must fail");
        assert_eq!(tamper_error, CheckpointError::SignatureVerificationFailed);

        let projection = ledger
            .projection(&fixture.transition_id)
            .cloned()
            .expect("projection must exist");
        assert_eq!(ledger.event_count(), 6);
        assert!(!projection.side_effect_committed);
        let final_dimensions = projection.dimensions.as_ref().expect("dimensions");
        assert_eq!(
            final_dimensions.causal_validity,
            CausalValidityState::NotEvaluated
        );
        assert_eq!(
            final_dimensions.continuity_posture,
            ContinuityPosture::ReportOnly
        );

        (checkpoint_one, checkpoint_two, anchor, projection)
    };

    let reopened = TrustworthyTransitionLedger::open(directory.path())?;
    assert_eq!(reopened.event_count(), 6);
    assert_eq!(
        reopened.projection(&fixture.transition_id),
        Some(&expected_projection)
    );

    let receipt = json!({
        "schema_version": "liminaldb-verified-negative-anchor-receipt-v0.1",
        "transition_id": fixture.transition_id,
        "checkpoint_count": 2,
        "first_checkpoint_ref": checkpoint_one.manifest_ref,
        "latest_checkpoint_ref": checkpoint_two.manifest_ref,
        "external_anchor_id": anchor.anchor_id,
        "latest_sequence": checkpoint_two.body.last_sequence,
        "signature_verified": true,
        "external_anchor_verified": true,
        "rollback_rejected": true,
        "fork_rejected": true,
        "tampered_signature_rejected": true,
        "restart_replay_equal": true,
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
            "production_write": false
        },
        "authority": {
            "external_submission": false,
            "deployment": false,
            "merge": false
        }
    });
    println!(
        "VERIFIED_NEGATIVE_ANCHOR_RECEIPT={}",
        serde_json::to_string(&receipt)?
    );
    Ok(())
}

#[test]
fn anchored_fixture_rejects_authority_and_memory_escalation() {
    let source = include_str!("fixtures/pythia_verified_negative_anchored_v0.1.json");

    let mut durable: Value = serde_json::from_str(source).expect("fixture JSON");
    durable["judgment"]["durable_memory"] = Value::Bool(true);
    let durable: Fixture = serde_json::from_value(durable).expect("typed durable fixture");
    assert!(validate_fixture(&durable).is_err());

    let mut external: Value = serde_json::from_str(source).expect("fixture JSON");
    external["authority"]["external_submission"] = Value::Bool(true);
    let external: Fixture = serde_json::from_value(external).expect("typed external fixture");
    assert!(validate_fixture(&external).is_err());

    let mut power_loss: Value = serde_json::from_str(source).expect("fixture JSON");
    power_loss["rehearsal_boundary"]["sudden_power_loss_claimed"] = Value::Bool(true);
    let power_loss: Fixture = serde_json::from_value(power_loss).expect("typed power fixture");
    assert!(validate_fixture(&power_loss).is_err());

    let mut source_drift: Value = serde_json::from_str(source).expect("fixture JSON");
    source_drift["source"]["transition_run_id"] = Value::from(0_u64);
    let source_drift: Fixture =
        serde_json::from_value(source_drift).expect("typed source-drift fixture");
    assert!(validate_fixture(&source_drift).is_err());

    let mut record_drift: Value = serde_json::from_str(source).expect("fixture JSON");
    record_drift["records"]["authorization"]["record_ref"] =
        Value::String("sha256:drift".to_owned());
    let record_drift: Fixture =
        serde_json::from_value(record_drift).expect("typed record-drift fixture");
    assert!(validate_fixture(&record_drift).is_err());
}
