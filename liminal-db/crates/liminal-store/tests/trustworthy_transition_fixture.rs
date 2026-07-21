use std::collections::BTreeSet;
use std::fs;

use crc32fast::Hasher as Crc32;
use liminal_store::{
    sha256_ref, AuthorityState, CausalValidityState, ContinuityPosture, ExecutionState, Offset,
    ResponseIntegrityState, Store, TransitionDimensions, TransitionEvent, TransitionEventInput,
    TransitionLedgerError, TransitionLinks, TransitionRecordKind, TrustworthyTransitionLedger,
};
use serde_cbor::Value as CborValue;
use serde_json::Value;
use tempfile::tempdir;

const FIXTURE: &str = include_str!("fixtures/trustworthy_transition_ledger_v0.1.json");

fn reference(label: &str) -> String {
    sha256_ref(label.as_bytes())
}

fn input(
    transition_id: &str,
    subject_id: &str,
    kind: TransitionRecordKind,
    label: &str,
    links: TransitionLinks,
) -> TransitionEventInput {
    TransitionEventInput {
        transition_id: transition_id.to_owned(),
        subject_id: subject_id.to_owned(),
        kind,
        record_ref: reference(&format!("record:{label}")),
        payload_digest: reference(&format!("payload:{label}")),
        links,
        dimensions: None,
        side_effect_committed: None,
        captured_at_ms: 1,
    }
}

fn dimensions(execution: ExecutionState) -> TransitionDimensions {
    TransitionDimensions {
        authority: AuthorityState::Valid,
        execution,
        response_integrity: ResponseIntegrityState::Verified,
        causal_validity: CausalValidityState::Valid,
        continuity_posture: ContinuityPosture::ReportOnly,
    }
}

fn append_authorization(
    ledger: &mut TrustworthyTransitionLedger,
    transition_id: &str,
    subject_id: &str,
    label: &str,
) -> TransitionEvent {
    ledger
        .append(input(
            transition_id,
            subject_id,
            TransitionRecordKind::Authorization,
            label,
            TransitionLinks::default(),
        ))
        .expect("authorization must append")
}

fn append_observation(
    ledger: &mut TrustworthyTransitionLedger,
    transition_id: &str,
    subject_id: &str,
    label: &str,
    authorization_ref: &str,
) -> TransitionEvent {
    ledger
        .append(input(
            transition_id,
            subject_id,
            TransitionRecordKind::Observation,
            label,
            TransitionLinks {
                authorization_ref: Some(authorization_ref.to_owned()),
                ..TransitionLinks::default()
            },
        ))
        .expect("observation must append")
}

fn append_full_chain(
    ledger: &mut TrustworthyTransitionLedger,
    transition_id: &str,
    subject_id: &str,
) -> Vec<TransitionEvent> {
    let authorization = append_authorization(ledger, transition_id, subject_id, "authorization");
    let observation = append_observation(
        ledger,
        transition_id,
        subject_id,
        "observation",
        &authorization.body.record_ref,
    );
    let integrity = ledger
        .append(input(
            transition_id,
            subject_id,
            TransitionRecordKind::ResponseIntegrity,
            "integrity",
            TransitionLinks {
                authorization_ref: Some(authorization.body.record_ref.clone()),
                observation_refs: vec![observation.body.record_ref.clone()],
                ..TransitionLinks::default()
            },
        ))
        .expect("integrity must append");
    let causal = ledger
        .append(input(
            transition_id,
            subject_id,
            TransitionRecordKind::CausalAudit,
            "causal",
            TransitionLinks {
                authorization_ref: Some(authorization.body.record_ref.clone()),
                observation_refs: vec![observation.body.record_ref.clone()],
                response_integrity_ref: Some(integrity.body.record_ref.clone()),
                ..TransitionLinks::default()
            },
        ))
        .expect("causal audit must append");
    let mut continuity_input = input(
        transition_id,
        subject_id,
        TransitionRecordKind::ContinuitySnapshot,
        "continuity",
        TransitionLinks {
            authorization_ref: Some(authorization.body.record_ref.clone()),
            observation_refs: vec![observation.body.record_ref.clone()],
            response_integrity_ref: Some(integrity.body.record_ref.clone()),
            causal_audit_ref: Some(causal.body.record_ref.clone()),
            ..TransitionLinks::default()
        },
    );
    continuity_input.dimensions = Some(dimensions(ExecutionState::ObservedExecuted));
    let continuity = ledger
        .append(continuity_input)
        .expect("continuity must append");
    vec![authorization, observation, integrity, causal, continuity]
}

fn error_code(error: &TransitionLedgerError) -> &'static str {
    match error {
        TransitionLedgerError::ParentMismatch(_) => "PARENT_MISMATCH",
        TransitionLedgerError::MissingParent(_) => "MISSING_PARENT",
        TransitionLedgerError::ObservationSetMismatch => "OBSERVATION_SET_MISMATCH",
        TransitionLedgerError::DuplicateRecordReference(_) => "DUPLICATE_RECORD_REFERENCE",
        TransitionLedgerError::SideEffectRollback => "SIDE_EFFECT_ROLLBACK",
        TransitionLedgerError::ExecutionRollback => "EXECUTION_ROLLBACK",
        TransitionLedgerError::ReauthorizationWithoutSupersession => {
            "REAUTHORIZATION_WITHOUT_SUPERSESSION"
        }
        TransitionLedgerError::SnapshotDigestMismatch => "SNAPSHOT_DIGEST_MISMATCH",
        TransitionLedgerError::EventHashMismatch => "EVENT_HASH_MISMATCH",
        other => panic!("unexpected fixture error: {other:?}"),
    }
}

fn execute_case(case_id: &str) -> Result<&'static str, &'static str> {
    match case_id {
        "valid_full_chain_restart" => {
            let root = tempdir().expect("tempdir");
            {
                let mut ledger = TrustworthyTransitionLedger::open(root.path()).expect("open");
                append_full_chain(&mut ledger, "transition-a", "subject-a");
            }
            let recovered = TrustworthyTransitionLedger::open(root.path()).expect("restart");
            assert_eq!(recovered.event_count(), 5);
            Ok("RECOVERED")
        }
        "cross_transition_parent_substitution" => {
            let root = tempdir().expect("tempdir");
            let mut ledger = TrustworthyTransitionLedger::open(root.path()).expect("open");
            let first = append_authorization(&mut ledger, "transition-a", "subject-a", "auth-a");
            let second = append_authorization(&mut ledger, "transition-b", "subject-b", "auth-b");
            assert_ne!(first.body.record_ref, second.body.record_ref);
            let error = ledger
                .append(input(
                    "transition-a",
                    "subject-a",
                    TransitionRecordKind::Observation,
                    "cross-parent",
                    TransitionLinks {
                        authorization_ref: Some(second.body.record_ref),
                        ..TransitionLinks::default()
                    },
                ))
                .expect_err("cross-transition parent must fail");
            Err(error_code(&error))
        }
        "missing_parent_reference" => {
            let root = tempdir().expect("tempdir");
            let mut ledger = TrustworthyTransitionLedger::open(root.path()).expect("open");
            append_authorization(&mut ledger, "transition-a", "subject-a", "auth-a");
            let error = ledger
                .append(input(
                    "transition-a",
                    "subject-a",
                    TransitionRecordKind::Observation,
                    "missing-parent",
                    TransitionLinks {
                        authorization_ref: Some(reference("does-not-exist")),
                        ..TransitionLinks::default()
                    },
                ))
                .expect_err("missing parent must fail");
            Err(error_code(&error))
        }
        "missing_observation_from_integrity_set" => {
            let root = tempdir().expect("tempdir");
            let mut ledger = TrustworthyTransitionLedger::open(root.path()).expect("open");
            let authorization =
                append_authorization(&mut ledger, "transition-a", "subject-a", "auth-a");
            append_observation(
                &mut ledger,
                "transition-a",
                "subject-a",
                "obs-a",
                &authorization.body.record_ref,
            );
            let error = ledger
                .append(input(
                    "transition-a",
                    "subject-a",
                    TransitionRecordKind::ResponseIntegrity,
                    "integrity-empty",
                    TransitionLinks {
                        authorization_ref: Some(authorization.body.record_ref),
                        observation_refs: Vec::new(),
                        ..TransitionLinks::default()
                    },
                ))
                .expect_err("incomplete observation set must fail");
            Err(error_code(&error))
        }
        "duplicate_global_record_ref" => {
            let root = tempdir().expect("tempdir");
            let mut ledger = TrustworthyTransitionLedger::open(root.path()).expect("open");
            let first = append_authorization(&mut ledger, "transition-a", "subject-a", "auth-a");
            let mut duplicate = input(
                "transition-b",
                "subject-b",
                TransitionRecordKind::Authorization,
                "auth-b",
                TransitionLinks::default(),
            );
            duplicate.record_ref = first.body.record_ref;
            let error = ledger
                .append(duplicate)
                .expect_err("duplicate global record ref must fail");
            Err(error_code(&error))
        }
        "side_effect_commitment_rollback" => {
            let root = tempdir().expect("tempdir");
            let mut ledger = TrustworthyTransitionLedger::open(root.path()).expect("open");
            let mut authorization = input(
                "transition-a",
                "subject-a",
                TransitionRecordKind::Authorization,
                "auth-a",
                TransitionLinks::default(),
            );
            authorization.side_effect_committed = Some(true);
            let authorization = ledger.append(authorization).expect("authorization");
            let mut observation = input(
                "transition-a",
                "subject-a",
                TransitionRecordKind::Observation,
                "obs-a",
                TransitionLinks {
                    authorization_ref: Some(authorization.body.record_ref),
                    ..TransitionLinks::default()
                },
            );
            observation.side_effect_committed = Some(false);
            let error = ledger
                .append(observation)
                .expect_err("side effect rollback must fail");
            Err(error_code(&error))
        }
        "execution_dimension_rollback" => {
            let root = tempdir().expect("tempdir");
            let mut ledger = TrustworthyTransitionLedger::open(root.path()).expect("open");
            let mut authorization = input(
                "transition-a",
                "subject-a",
                TransitionRecordKind::Authorization,
                "auth-a",
                TransitionLinks::default(),
            );
            authorization.dimensions = Some(dimensions(ExecutionState::ObservedExecuted));
            let authorization = ledger.append(authorization).expect("authorization");
            let mut observation = input(
                "transition-a",
                "subject-a",
                TransitionRecordKind::Observation,
                "obs-a",
                TransitionLinks {
                    authorization_ref: Some(authorization.body.record_ref),
                    ..TransitionLinks::default()
                },
            );
            observation.dimensions = Some(dimensions(ExecutionState::NotObserved));
            let error = ledger
                .append(observation)
                .expect_err("execution rollback must fail");
            Err(error_code(&error))
        }
        "implicit_reauthorization" => {
            let root = tempdir().expect("tempdir");
            let mut ledger = TrustworthyTransitionLedger::open(root.path()).expect("open");
            append_authorization(&mut ledger, "transition-a", "subject-a", "auth-a");
            let error = ledger
                .append(input(
                    "transition-a",
                    "subject-a",
                    TransitionRecordKind::Authorization,
                    "auth-b",
                    TransitionLinks::default(),
                ))
                .expect_err("implicit reauthorization must fail");
            Err(error_code(&error))
        }
        "tampered_snapshot_digest" => {
            let root = tempdir().expect("tempdir");
            let snapshot_path = {
                let mut ledger = TrustworthyTransitionLedger::open(root.path()).expect("open");
                append_authorization(&mut ledger, "transition-a", "subject-a", "auth-a");
                ledger.write_snapshot(2).expect("snapshot").path
            };
            let bytes = fs::read(&snapshot_path).expect("read snapshot");
            let mut value: CborValue = serde_cbor::from_slice(&bytes).expect("snapshot CBOR");
            let CborValue::Map(map) = &mut value else {
                panic!("snapshot must be a CBOR map");
            };
            map.insert(
                CborValue::Text("snapshot_digest".to_owned()),
                CborValue::Text(reference("tampered-snapshot")),
            );
            fs::write(
                &snapshot_path,
                serde_cbor::to_vec(&value).expect("encode snapshot"),
            )
            .expect("write snapshot");
            let error = TrustworthyTransitionLedger::open(root.path())
                .err()
                .expect("tampered snapshot must fail");
            Err(error_code(&error))
        }
        "tampered_semantic_event_hash" => {
            let root = tempdir().expect("tempdir");
            {
                let mut ledger = TrustworthyTransitionLedger::open(root.path()).expect("open");
                append_authorization(&mut ledger, "transition-a", "subject-a", "auth-a");
            }
            let wal_path = root.path().join("data/00000001.wal");
            let bytes = fs::read(&wal_path).expect("read WAL");
            let payload_len = u32::from_le_bytes(bytes[0..4].try_into().expect("length")) as usize;
            let mut event: TransitionEvent =
                serde_cbor::from_slice(&bytes[4..4 + payload_len]).expect("event CBOR");
            event.event_hash = reference("tampered-event-hash");
            let payload = serde_cbor::to_vec(&event).expect("encode event");
            let mut crc = Crc32::new();
            crc.update(&payload);
            let mut frame = Vec::with_capacity(payload.len() + 8);
            frame.extend_from_slice(&(payload.len() as u32).to_le_bytes());
            frame.extend_from_slice(&payload);
            frame.extend_from_slice(&crc.finalize().to_le_bytes());
            fs::write(&wal_path, frame).expect("write WAL");
            let error = TrustworthyTransitionLedger::open(root.path())
                .err()
                .expect("tampered event must fail");
            Err(error_code(&error))
        }
        "snapshot_tail_equals_full_replay" => {
            let root = tempdir().expect("tempdir");
            {
                let mut ledger = TrustworthyTransitionLedger::open(root.path()).expect("open");
                let authorization =
                    append_authorization(&mut ledger, "transition-a", "subject-a", "auth-a");
                append_observation(
                    &mut ledger,
                    "transition-a",
                    "subject-a",
                    "obs-a",
                    &authorization.body.record_ref,
                );
                ledger.write_snapshot(2).expect("snapshot");
                append_observation(
                    &mut ledger,
                    "transition-a",
                    "subject-a",
                    "obs-b",
                    &authorization.body.record_ref,
                );
            }
            let recovered = TrustworthyTransitionLedger::open(root.path()).expect("restart");
            assert_eq!(recovered.event_count(), 3);
            assert_eq!(
                recovered
                    .projection("transition-a")
                    .expect("projection")
                    .observation_refs
                    .len(),
                2
            );
            Ok("RECOVERED")
        }
        "wal_rotation_round_trip" => {
            let root = tempdir().expect("tempdir");
            let mut store = Store::open(root.path()).expect("open store");
            for index in 0u8..33 {
                let payload = vec![index; 1024 * 1024];
                store.append(&payload).expect("append rotation payload");
            }
            assert!(store.current_segment() > 1, "WAL must rotate");
            let mut stream = store.stream_from(Offset::start()).expect("stream WAL");
            for index in 0u8..33 {
                let payload = stream
                    .next()
                    .expect("record must exist")
                    .expect("record must decode");
                assert_eq!(payload.len(), 1024 * 1024);
                assert_eq!(payload[0], index);
            }
            assert!(stream.next().is_none(), "no extra records");
            Ok("RECOVERED")
        }
        other => panic!("fixture case has no executable scenario: {other}"),
    }
}

#[test]
fn trustworthy_transition_fixture_is_versioned_and_executable() {
    let fixture: Value = serde_json::from_str(FIXTURE).expect("fixture must be valid JSON");
    assert_eq!(fixture["fixture_version"], "0.1");
    assert_eq!(
        fixture["profile"],
        "org.liminaldb.trustworthy-transition-ledger.v0.1"
    );

    let cases = fixture["cases"].as_array().expect("cases array");
    assert_eq!(cases.len(), 12);

    let expected_errors: BTreeSet<&str> = [
        "PARENT_MISMATCH",
        "MISSING_PARENT",
        "OBSERVATION_SET_MISMATCH",
        "DUPLICATE_RECORD_REFERENCE",
        "SIDE_EFFECT_ROLLBACK",
        "EXECUTION_ROLLBACK",
        "REAUTHORIZATION_WITHOUT_SUPERSESSION",
        "SNAPSHOT_DIGEST_MISMATCH",
        "EVENT_HASH_MISMATCH",
    ]
    .into_iter()
    .collect();

    let mut ids = BTreeSet::new();
    for case in cases {
        let case_id = case["case_id"]
            .as_str()
            .expect("every case must have case_id");
        assert!(ids.insert(case_id), "duplicate case_id: {case_id}");

        match execute_case(case_id) {
            Ok(actual) => assert_eq!(
                case["expected"].as_str(),
                Some(actual),
                "case {case_id} returned an unexpected outcome"
            ),
            Err(actual) => {
                assert!(expected_errors.contains(actual));
                assert_eq!(
                    case["expected_error"].as_str(),
                    Some(actual),
                    "case {case_id} returned an unexpected stable error"
                );
            }
        }
    }
}

#[test]
fn observation_growth_invalidates_all_derived_current_evidence() {
    let root = tempdir().expect("tempdir");
    let mut ledger = TrustworthyTransitionLedger::open(root.path()).expect("open");
    let chain = append_full_chain(&mut ledger, "transition-a", "subject-a");
    let authorization = &chain[0];
    let integrity = &chain[2];
    let causal = &chain[3];
    let continuity = &chain[4];

    let second_observation = append_observation(
        &mut ledger,
        "transition-a",
        "subject-a",
        "observation-b",
        &authorization.body.record_ref,
    );
    let projection = ledger.projection("transition-a").expect("projection");
    assert_eq!(projection.observation_refs.len(), 2);
    assert!(projection.response_integrity_ref.is_none());
    assert!(projection.causal_audit_ref.is_none());
    assert!(projection.continuity_snapshot_ref.is_none());

    let mut stale_continuity = input(
        "transition-a",
        "subject-a",
        TransitionRecordKind::ContinuitySnapshot,
        "stale-continuity",
        TransitionLinks {
            authorization_ref: Some(authorization.body.record_ref.clone()),
            observation_refs: vec![
                chain[1].body.record_ref.clone(),
                second_observation.body.record_ref,
            ],
            response_integrity_ref: Some(integrity.body.record_ref.clone()),
            causal_audit_ref: Some(causal.body.record_ref.clone()),
            previous_continuity_ref: Some(continuity.body.record_ref.clone()),
        },
    );
    stale_continuity.dimensions = Some(dimensions(ExecutionState::ObservedExecuted));
    assert!(matches!(
        ledger.append(stale_continuity),
        Err(TransitionLedgerError::ParentMismatch(
            "response_integrity_ref"
        ))
    ));
}
