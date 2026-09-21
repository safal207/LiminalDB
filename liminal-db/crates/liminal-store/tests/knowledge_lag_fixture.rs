use liminal_store::{
    sha256_ref, AuthorityState, CausalValidityState, ContinuityPosture, ExecutionState,
    ResponseIntegrityState, TransitionDimensions, TransitionEventInput, TransitionLinks,
    TransitionRecordKind, TrustworthyTransitionLedger,
};
use serde_json::Value;
use tempfile::tempdir;

const FIXTURE: &str = include_str!("fixtures/knowledge_lag_v0.1.json");

fn reference(label: &str) -> String {
    sha256_ref(label.as_bytes())
}

fn input(
    kind: TransitionRecordKind,
    label: &str,
    links: TransitionLinks,
) -> TransitionEventInput {
    TransitionEventInput {
        transition_id: "knowledge-lag:retroactive-readback".to_owned(),
        subject_id: "source-fixture:knowledge-lag".to_owned(),
        kind,
        record_ref: reference(&format!("record:{label}")),
        payload_digest: reference(&format!("payload:{label}")),
        links,
        dimensions: None,
        side_effect_committed: None,
        captured_at_ms: 1,
    }
}

fn authority(value: &str) -> AuthorityState {
    match value {
        "CONSUMED" => AuthorityState::Consumed,
        other => panic!("unsupported authority state: {other}"),
    }
}

fn execution(value: &str) -> ExecutionState {
    match value {
        "NOT_OBSERVED" => ExecutionState::NotObserved,
        "OBSERVED_EXECUTED" => ExecutionState::ObservedExecuted,
        other => panic!("unsupported execution state: {other}"),
    }
}

fn integrity(value: &str) -> ResponseIntegrityState {
    match value {
        "UNKNOWN" => ResponseIntegrityState::Unknown,
        "VERIFIED" => ResponseIntegrityState::Verified,
        other => panic!("unsupported response integrity: {other}"),
    }
}

fn causal(value: &str) -> CausalValidityState {
    match value {
        "NOT_EVALUATED" => CausalValidityState::NotEvaluated,
        "VALID" => CausalValidityState::Valid,
        other => panic!("unsupported causal state: {other}"),
    }
}

fn posture(value: &str) -> ContinuityPosture {
    match value {
        "REVALIDATE" => ContinuityPosture::Revalidate,
        "REPORT_ONLY" => ContinuityPosture::ReportOnly,
        other => panic!("unsupported continuity posture: {other}"),
    }
}

fn dimensions(liminal: &Value) -> TransitionDimensions {
    TransitionDimensions {
        authority: authority(liminal["authority"].as_str().expect("authority")),
        execution: execution(liminal["execution"].as_str().expect("execution")),
        response_integrity: integrity(
            liminal["response_integrity"]
                .as_str()
                .expect("response_integrity"),
        ),
        causal_validity: causal(
            liminal["causal_validity"]
                .as_str()
                .expect("causal_validity"),
        ),
        continuity_posture: posture(
            liminal["continuity_posture"]
                .as_str()
                .expect("continuity_posture"),
        ),
    }
}

fn append_derived_chain(
    ledger: &mut TrustworthyTransitionLedger,
    authorization_ref: &str,
    observation_refs: Vec<String>,
    label: &str,
    expected: &TransitionDimensions,
    side_effect_committed: Option<bool>,
) {
    let mut response = input(
        TransitionRecordKind::ResponseIntegrity,
        &format!("{label}:response"),
        TransitionLinks {
            authorization_ref: Some(authorization_ref.to_owned()),
            observation_refs: observation_refs.clone(),
            ..TransitionLinks::default()
        },
    );
    response.dimensions = Some(TransitionDimensions {
        authority: expected.authority,
        execution: expected.execution,
        response_integrity: expected.response_integrity,
        causal_validity: CausalValidityState::NotEvaluated,
        continuity_posture: ContinuityPosture::NotEvaluated,
    });
    response.side_effect_committed = side_effect_committed;
    let response = ledger.append(response).expect("response integrity");

    let mut causal_event = input(
        TransitionRecordKind::CausalAudit,
        &format!("{label}:causal"),
        TransitionLinks {
            authorization_ref: Some(authorization_ref.to_owned()),
            observation_refs: observation_refs.clone(),
            response_integrity_ref: Some(response.body.record_ref.clone()),
            ..TransitionLinks::default()
        },
    );
    causal_event.dimensions = Some(TransitionDimensions {
        authority: expected.authority,
        execution: expected.execution,
        response_integrity: expected.response_integrity,
        causal_validity: expected.causal_validity,
        continuity_posture: ContinuityPosture::NotEvaluated,
    });
    causal_event.side_effect_committed = side_effect_committed;
    let causal_event = ledger.append(causal_event).expect("causal audit");

    let mut continuity = input(
        TransitionRecordKind::ContinuitySnapshot,
        &format!("{label}:continuity"),
        TransitionLinks {
            authorization_ref: Some(authorization_ref.to_owned()),
            observation_refs,
            response_integrity_ref: Some(response.body.record_ref),
            causal_audit_ref: Some(causal_event.body.record_ref),
            previous_continuity_ref: None,
        },
    );
    continuity.dimensions = Some(expected.clone());
    continuity.side_effect_committed = side_effect_committed;
    ledger.append(continuity).expect("continuity");
}

#[test]
fn later_readback_changes_current_conclusion_without_rewriting_prior_knowledge() {
    let fixture: Value = serde_json::from_str(FIXTURE).expect("fixture JSON");
    assert_eq!(fixture["system_case"], "KNOWLEDGE-LAG-001");

    let revisions = fixture["expected"]["revisions"]
        .as_array()
        .expect("revisions");
    assert_eq!(revisions.len(), 3);

    let root = tempdir().expect("tempdir");
    let mut ledger = TrustworthyTransitionLedger::open(root.path()).expect("open");

    let authorization = ledger
        .append(input(
            TransitionRecordKind::Authorization,
            "authorization",
            TransitionLinks::default(),
        ))
        .expect("authorization");

    // T2 / CLAIM_ONLY: no external observation exists yet.
    let claim_expected = dimensions(&revisions[0]["liminal"]);
    append_derived_chain(
        &mut ledger,
        &authorization.body.record_ref,
        vec![],
        "claim-only",
        &claim_expected,
        None,
    );
    let claim_projection = ledger
        .projection("knowledge-lag:retroactive-readback")
        .expect("claim projection")
        .clone();
    assert_eq!(claim_projection.dimensions.as_ref(), Some(&claim_expected));
    assert!(!claim_projection.side_effect_committed);
    assert!(claim_projection.observation_refs.is_empty());

    // T3 / READBACK_UNAVAILABLE: a real readback attempt exists, but it cannot see effect truth.
    let unavailable_expected = dimensions(&revisions[1]["liminal"]);
    let mut unavailable_observation = input(
        TransitionRecordKind::Observation,
        "unavailable-observation",
        TransitionLinks {
            authorization_ref: Some(authorization.body.record_ref.clone()),
            ..TransitionLinks::default()
        },
    );
    unavailable_observation.dimensions = Some(TransitionDimensions {
        authority: unavailable_expected.authority,
        execution: unavailable_expected.execution,
        response_integrity: ResponseIntegrityState::NotEvaluated,
        causal_validity: CausalValidityState::NotEvaluated,
        continuity_posture: ContinuityPosture::NotEvaluated,
    });
    let unavailable_observation = ledger
        .append(unavailable_observation)
        .expect("unavailable observation");

    let invalidated_after_t3 = ledger
        .projection("knowledge-lag:retroactive-readback")
        .expect("projection after T3 observation");
    assert!(
        invalidated_after_t3.continuity_snapshot_ref.is_none(),
        "new T3 observation must invalidate the claim-only continuity"
    );

    append_derived_chain(
        &mut ledger,
        &authorization.body.record_ref,
        vec![unavailable_observation.body.record_ref.clone()],
        "readback-unavailable",
        &unavailable_expected,
        None,
    );
    let unavailable_projection = ledger
        .projection("knowledge-lag:retroactive-readback")
        .expect("unavailable projection")
        .clone();
    assert_eq!(
        unavailable_projection.dimensions.as_ref(),
        Some(&unavailable_expected)
    );
    assert!(!unavailable_projection.side_effect_committed);
    assert_eq!(unavailable_projection.observation_refs.len(), 1);

    // T4 / READBACK_FULL: a second observation sees the old T1 effect.
    let full_expected = dimensions(&revisions[2]["liminal"]);
    let mut full_observation = input(
        TransitionRecordKind::Observation,
        "full-observation",
        TransitionLinks {
            authorization_ref: Some(authorization.body.record_ref.clone()),
            ..TransitionLinks::default()
        },
    );
    full_observation.dimensions = Some(TransitionDimensions {
        authority: full_expected.authority,
        execution: full_expected.execution,
        response_integrity: ResponseIntegrityState::NotEvaluated,
        causal_validity: CausalValidityState::NotEvaluated,
        continuity_posture: ContinuityPosture::NotEvaluated,
    });
    full_observation.side_effect_committed = Some(true);
    let full_observation = ledger.append(full_observation).expect("full observation");

    let invalidated_after_t4 = ledger
        .projection("knowledge-lag:retroactive-readback")
        .expect("projection after T4 observation");
    assert!(
        invalidated_after_t4.continuity_snapshot_ref.is_none(),
        "new T4 observation must invalidate the T3 continuity"
    );

    append_derived_chain(
        &mut ledger,
        &authorization.body.record_ref,
        vec![
            unavailable_observation.body.record_ref,
            full_observation.body.record_ref,
        ],
        "readback-full",
        &full_expected,
        Some(true),
    );

    let final_projection = ledger
        .projection("knowledge-lag:retroactive-readback")
        .expect("final projection")
        .clone();
    assert_eq!(final_projection.dimensions.as_ref(), Some(&full_expected));
    assert!(final_projection.side_effect_committed);
    assert_eq!(final_projection.observation_refs.len(), 2);

    ledger.write_snapshot(2).expect("snapshot");
    drop(ledger);

    let reopened = TrustworthyTransitionLedger::open(root.path()).expect("reopen");
    let recovered = reopened
        .projection("knowledge-lag:retroactive-readback")
        .expect("recovered projection");
    assert_eq!(recovered, &final_projection);

    println!(
        "KNOWLEDGE-LAG-001 claim={:?}/{:?} unavailable={:?}/{:?} full={:?}/{:?}",
        claim_expected.execution,
        claim_expected.continuity_posture,
        unavailable_expected.execution,
        unavailable_expected.continuity_posture,
        full_expected.execution,
        full_expected.continuity_posture,
    );
}

#[test]
fn unavailable_never_becomes_no_effect_or_retry_authority() {
    let fixture: Value = serde_json::from_str(FIXTURE).expect("fixture JSON");
    let revisions = fixture["expected"]["revisions"]
        .as_array()
        .expect("revisions");

    let unavailable = &revisions[1];
    assert_eq!(
        unavailable["source"]["observation_availability"],
        "UNAVAILABLE"
    );
    assert_eq!(unavailable["source"]["external_outcome"], "INDETERMINATE");
    assert_eq!(unavailable["source"]["externally_verified"], false);
    assert!(unavailable["source"]["effect_count"].is_null());
    assert_eq!(
        unavailable["liminal"]["continuity_posture"],
        "REVALIDATE"
    );
    assert_eq!(unavailable["liminal"]["authority"], "CONSUMED");
    assert!(unavailable["liminal"]["side_effect_committed"].is_null());
}
