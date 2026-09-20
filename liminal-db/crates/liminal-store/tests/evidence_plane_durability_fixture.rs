use liminal_store::{
    sha256_ref, AuthorityState, CausalValidityState, ContinuityPosture, ExecutionState,
    ResponseIntegrityState, TransitionDimensions, TransitionEvent, TransitionEventInput,
    TransitionLinks, TransitionRecordKind, TrustworthyTransitionLedger,
};
use serde_json::Value;
use tempfile::tempdir;

const FIXTURE: &str =
    include_str!("fixtures/evidence_plane_durability_v0.1.json");

fn reference(label: &str) -> String {
    sha256_ref(label.as_bytes())
}

fn event(
    case_id: &str,
    kind: TransitionRecordKind,
    label: &str,
    links: TransitionLinks,
) -> TransitionEventInput {
    TransitionEventInput {
        transition_id: format!("durability:{case_id}"),
        subject_id: "external-proof:safal207/ContractGraph-QA".to_owned(),
        kind,
        record_ref: reference(&format!("{case_id}:record:{label}")),
        payload_digest: reference(&format!("{case_id}:payload:{label}")),
        links,
        dimensions: None,
        side_effect_committed: None,
        captured_at_ms: 1,
    }
}

fn authority(value: &str) -> AuthorityState {
    match value {
        "CONSUMED" => AuthorityState::Consumed,
        "REVALIDATION_REQUIRED" => AuthorityState::RevalidationRequired,
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
        "VERIFIED" => ResponseIntegrityState::Verified,
        "NOT_EVALUATED" => ResponseIntegrityState::NotEvaluated,
        "UNKNOWN" => ResponseIntegrityState::Unknown,
        other => panic!("unsupported response integrity state: {other}"),
    }
}

fn causal(value: &str) -> CausalValidityState {
    match value {
        "VALID" => CausalValidityState::Valid,
        "NOT_EVALUATED" => CausalValidityState::NotEvaluated,
        other => panic!("unsupported causal validity state: {other}"),
    }
}

fn posture(value: &str) -> ContinuityPosture {
    match value {
        "REPORT_ONLY" => ContinuityPosture::ReportOnly,
        "REVALIDATE" => ContinuityPosture::Revalidate,
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

fn source_contract(case: &Value) {
    let case_id = case["case_id"].as_str().expect("case_id");
    let revisions = case["revisions"].as_array().expect("revisions");

    assert_eq!(
        case["expected_attempt_count"], 1,
        "{case_id}: recovery must never add a provider attempt"
    );

    if case_id == "effect_then_crash" {
        assert_eq!(case["expected_effect_count"], 1);
        assert_eq!(revisions.len(), 1);
        assert_eq!(
            revisions[0]["source"]["external_outcome"],
            "ONE_EFFECT_MATCHING"
        );
        assert_eq!(
            revisions[0]["liminal"]["continuity_posture"],
            "REPORT_ONLY"
        );
        assert_eq!(revisions[0]["liminal"]["authority"], "CONSUMED");
    }

    if case_id == "crash_before_effect" {
        assert_eq!(case["expected_effect_count"], 0);
        assert_eq!(revisions.len(), 1);
        assert_eq!(revisions[0]["source"]["external_outcome"], "NO_EFFECT");
        assert_eq!(
            revisions[0]["source"]["recovery_decision"],
            "NO_REDISPATCH_FRESH_AUTHORIZATION_REQUIRED"
        );
        assert_eq!(
            revisions[0]["liminal"]["continuity_posture"],
            "REVALIDATE"
        );
        assert_eq!(
            revisions[0]["liminal"]["authority"],
            "REVALIDATION_REQUIRED"
        );
    }

    if case_id == "unavailable_readback" {
        assert_eq!(case["expected_effect_count"], 1);
        assert_eq!(revisions.len(), 2);
        assert_eq!(
            revisions[0]["source"]["observation_availability"],
            "UNAVAILABLE"
        );
        assert_eq!(
            revisions[0]["source"]["external_outcome"],
            "INDETERMINATE"
        );
        assert_eq!(revisions[0]["source"]["externally_verified"], false);
        assert!(revisions[0]["source"]["effect_count"].is_null());
        assert_eq!(
            revisions[0]["liminal"]["continuity_posture"],
            "REVALIDATE"
        );
        assert!(revisions[0]["liminal"]["side_effect_committed"].is_null());

        assert_eq!(
            revisions[1]["source"]["observation_availability"],
            "FULL"
        );
        assert_eq!(
            revisions[1]["source"]["external_outcome"],
            "ONE_EFFECT_MATCHING"
        );
        assert_eq!(
            revisions[1]["liminal"]["continuity_posture"],
            "REPORT_ONLY"
        );
    }
}

fn append_case(case: &Value) {
    source_contract(case);

    let case_id = case["case_id"].as_str().expect("case_id");
    let revisions = case["revisions"].as_array().expect("revisions");
    let root = tempdir().expect("tempdir");

    let mut expected_history: Vec<TransitionDimensions> = Vec::new();

    {
        let mut ledger =
            TrustworthyTransitionLedger::open(root.path()).expect("open ledger");

        let authorization = ledger
            .append(event(
                case_id,
                TransitionRecordKind::Authorization,
                "authorization",
                TransitionLinks::default(),
            ))
            .expect("authorization");

        let mut observation_refs: Vec<String> = Vec::new();
        let mut previous_continuity_ref: Option<String> = None;

        for (index, revision) in revisions.iter().enumerate() {
            let liminal = &revision["liminal"];
            let expected = dimensions(liminal);
            expected_history.push(expected.clone());
            let side_effect_committed =
                liminal["side_effect_committed"].as_bool();

            let mut observation = event(
                case_id,
                TransitionRecordKind::Observation,
                &format!("revision-{index}-observation"),
                TransitionLinks {
                    authorization_ref: Some(
                        authorization.body.record_ref.clone(),
                    ),
                    ..TransitionLinks::default()
                },
            );
            observation.dimensions = Some(TransitionDimensions {
                authority: expected.authority,
                execution: expected.execution,
                response_integrity:
                    ResponseIntegrityState::NotEvaluated,
                causal_validity:
                    CausalValidityState::NotEvaluated,
                continuity_posture:
                    ContinuityPosture::NotEvaluated,
            });
            observation.side_effect_committed = side_effect_committed;
            let observation =
                ledger.append(observation).expect("observation");
            observation_refs.push(observation.body.record_ref.clone());

            let mut response = event(
                case_id,
                TransitionRecordKind::ResponseIntegrity,
                &format!("revision-{index}-response"),
                TransitionLinks {
                    authorization_ref: Some(
                        authorization.body.record_ref.clone(),
                    ),
                    observation_refs: observation_refs.clone(),
                    ..TransitionLinks::default()
                },
            );
            response.dimensions = Some(TransitionDimensions {
                authority: expected.authority,
                execution: expected.execution,
                response_integrity: expected.response_integrity,
                causal_validity:
                    CausalValidityState::NotEvaluated,
                continuity_posture:
                    ContinuityPosture::NotEvaluated,
            });
            response.side_effect_committed = side_effect_committed;
            let response = ledger.append(response).expect("response");

            let mut causal_event = event(
                case_id,
                TransitionRecordKind::CausalAudit,
                &format!("revision-{index}-causal"),
                TransitionLinks {
                    authorization_ref: Some(
                        authorization.body.record_ref.clone(),
                    ),
                    observation_refs: observation_refs.clone(),
                    response_integrity_ref: Some(
                        response.body.record_ref.clone(),
                    ),
                    ..TransitionLinks::default()
                },
            );
            causal_event.dimensions = Some(TransitionDimensions {
                authority: expected.authority,
                execution: expected.execution,
                response_integrity: expected.response_integrity,
                causal_validity: expected.causal_validity,
                continuity_posture:
                    ContinuityPosture::NotEvaluated,
            });
            causal_event.side_effect_committed = side_effect_committed;
            let causal_event =
                ledger.append(causal_event).expect("causal audit");

            let mut continuity = event(
                case_id,
                TransitionRecordKind::ContinuitySnapshot,
                &format!("revision-{index}-continuity"),
                TransitionLinks {
                    authorization_ref: Some(
                        authorization.body.record_ref.clone(),
                    ),
                    observation_refs: observation_refs.clone(),
                    response_integrity_ref: Some(
                        response.body.record_ref.clone(),
                    ),
                    causal_audit_ref: Some(
                        causal_event.body.record_ref.clone(),
                    ),
                    previous_continuity_ref:
                        previous_continuity_ref.clone(),
                },
            );
            continuity.dimensions = Some(expected.clone());
            continuity.side_effect_committed = side_effect_committed;
            let continuity =
                ledger.append(continuity).expect("continuity");
            previous_continuity_ref =
                Some(continuity.body.record_ref.clone());

            let current = ledger
                .projection(&format!("durability:{case_id}"))
                .expect("projection");
            assert_eq!(
                current.dimensions.as_ref(),
                Some(&expected),
                "{case_id}: revision {index} dimensions"
            );

            if case_id == "unavailable_readback" && index == 0 {
                assert_eq!(
                    current
                        .dimensions
                        .as_ref()
                        .expect("dimensions")
                        .continuity_posture,
                    ContinuityPosture::Revalidate
                );
                assert!(
                    !current.side_effect_committed,
                    "unavailable evidence must not become committed-effect truth"
                );
            }
        }

        ledger.write_snapshot(2).expect("snapshot");
    }

    let reopened =
        TrustworthyTransitionLedger::open(root.path()).expect("reopen");
    let recovered = reopened
        .projection(&format!("durability:{case_id}"))
        .expect("recovered projection");

    let final_expected = expected_history.last().expect("final expected");
    assert_eq!(
        recovered.dimensions.as_ref(),
        Some(final_expected),
        "{case_id}: replay drift"
    );

    if case_id == "effect_then_crash" || case_id == "unavailable_readback" {
        assert!(
            recovered.side_effect_committed,
            "{case_id}: final matching effect must be committed"
        );
    }

    if case_id == "crash_before_effect" {
        assert!(
            !recovered.side_effect_committed,
            "verified no-effect must remain false"
        );
        assert_eq!(
            recovered
                .dimensions
                .as_ref()
                .expect("dimensions")
                .authority,
            AuthorityState::RevalidationRequired
        );
        assert_eq!(
            recovered
                .dimensions
                .as_ref()
                .expect("dimensions")
                .continuity_posture,
            ContinuityPosture::Revalidate
        );
    }

    println!(
        "EVIDENCE-PLANE-DURABILITY-001 case={case_id} revisions={} final_authority={:?} final_posture={:?} committed={}",
        revisions.len(),
        final_expected.authority,
        final_expected.continuity_posture,
        recovered.side_effect_committed
    );
}

#[test]
fn durable_crash_evidence_maps_to_replayable_liminaldb_continuity() {
    let fixture: Value = serde_json::from_str(FIXTURE).expect("fixture JSON");
    assert_eq!(fixture["fixture_version"], "0.1");
    assert_eq!(
        fixture["system_case"],
        "EVIDENCE-PLANE-DURABILITY-001"
    );
    assert_eq!(fixture["source"]["case_count"], 3);
    assert_eq!(
        fixture["source"]["commit"],
        "aef57d1059513b0de1c7cc5945777949175b44de"
    );

    let cases = fixture["cases"].as_array().expect("cases");
    assert_eq!(cases.len(), 3);

    for case in cases {
        append_case(case);
    }
}

#[test]
fn verified_no_effect_requires_revalidation_not_retry_side_effect() {
    let fixture: Value = serde_json::from_str(FIXTURE).expect("fixture JSON");
    let case = fixture["cases"]
        .as_array()
        .expect("cases")
        .iter()
        .find(|case| case["case_id"] == "crash_before_effect")
        .expect("crash_before_effect");

    let revision = &case["revisions"][0];
    assert_eq!(revision["source"]["external_outcome"], "NO_EFFECT");
    assert_eq!(
        revision["source"]["recovery_decision"],
        "NO_REDISPATCH_FRESH_AUTHORIZATION_REQUIRED"
    );
    assert_eq!(
        revision["liminal"]["authority"],
        "REVALIDATION_REQUIRED"
    );
    assert_eq!(
        revision["liminal"]["continuity_posture"],
        "REVALIDATE"
    );
}

#[test]
fn unavailable_readback_stays_indeterminate_until_full_readback() {
    let fixture: Value = serde_json::from_str(FIXTURE).expect("fixture JSON");
    let case = fixture["cases"]
        .as_array()
        .expect("cases")
        .iter()
        .find(|case| case["case_id"] == "unavailable_readback")
        .expect("unavailable_readback");

    let revisions = case["revisions"].as_array().expect("revisions");
    assert_eq!(revisions.len(), 2);

    assert_eq!(
        revisions[0]["source"]["observation_availability"],
        "UNAVAILABLE"
    );
    assert_eq!(
        revisions[0]["source"]["external_outcome"],
        "INDETERMINATE"
    );
    assert_eq!(
        revisions[0]["liminal"]["continuity_posture"],
        "REVALIDATE"
    );

    assert_eq!(
        revisions[1]["source"]["observation_availability"],
        "FULL"
    );
    assert_eq!(
        revisions[1]["source"]["external_outcome"],
        "ONE_EFFECT_MATCHING"
    );
    assert_eq!(
        revisions[1]["liminal"]["continuity_posture"],
        "REPORT_ONLY"
    );
}
