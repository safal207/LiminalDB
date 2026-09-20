use liminal_store::{
    sha256_ref, AuthorityState, CausalValidityState, ContinuityPosture, ExecutionState,
    ResponseIntegrityState, TransitionDimensions, TransitionEvent, TransitionEventInput,
    TransitionLinks, TransitionRecordKind, TrustworthyTransitionLedger,
};
use serde_json::Value;
use tempfile::tempdir;

const FIXTURE: &str = include_str!("fixtures/crashpoint_action_readback_v0.1.json");

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
        transition_id: format!("crashpoint:{case_id}"),
        subject_id: "external-fixture:mstevens843/crashpoint".to_owned(),
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
        "VALID" => AuthorityState::Valid,
        other => panic!("unsupported authority state in fixture: {other}"),
    }
}

fn execution(value: &str) -> ExecutionState {
    match value {
        "NOT_OBSERVED" => ExecutionState::NotObserved,
        "OBSERVED_EXECUTED" => ExecutionState::ObservedExecuted,
        other => panic!("unsupported execution state in fixture: {other}"),
    }
}

fn integrity(value: &str) -> ResponseIntegrityState {
    match value {
        "VERIFIED" => ResponseIntegrityState::Verified,
        "FAILED" => ResponseIntegrityState::Failed,
        "PARTIAL" => ResponseIntegrityState::Partial,
        "NOT_EVALUATED" => ResponseIntegrityState::NotEvaluated,
        "UNKNOWN" => ResponseIntegrityState::Unknown,
        other => panic!("unsupported response-integrity state in fixture: {other}"),
    }
}

fn causal(value: &str) -> CausalValidityState {
    match value {
        "VALID" => CausalValidityState::Valid,
        "INVALID" => CausalValidityState::Invalid,
        "NOT_EVALUATED" => CausalValidityState::NotEvaluated,
        other => panic!("unsupported causal-validity state in fixture: {other}"),
    }
}

fn posture(value: &str) -> ContinuityPosture {
    match value {
        "REPORT_ONLY" => ContinuityPosture::ReportOnly,
        "RETRY_SIDE_EFFECT" => ContinuityPosture::RetrySideEffect,
        "BLOCKED" => ContinuityPosture::Blocked,
        "REVALIDATE" => ContinuityPosture::Revalidate,
        other => panic!("unsupported continuity posture in fixture: {other}"),
    }
}

fn expected_dimensions(liminal: &Value) -> TransitionDimensions {
    TransitionDimensions {
        authority: authority("VALID"),
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
    let source = &case["source"];
    let liminal = &case["liminal"];

    if source["observation_availability"] == "UNAVAILABLE" {
        assert_eq!(source["external_outcome"], "INDETERMINATE", "{case_id}");
        assert_eq!(source["externally_verified"], false, "{case_id}");
        assert!(source["effect_count"].is_null(), "{case_id}");
        assert_eq!(liminal["continuity_posture"], "REVALIDATE", "{case_id}");
        assert!(
            liminal["side_effect_committed"].is_null(),
            "{case_id}: unavailable readback must not be upgraded to a committed or absent effect"
        );
    }

    if source["external_outcome"] == "NO_EFFECT" {
        assert_eq!(source["externally_verified"], true, "{case_id}");
        assert_eq!(source["effect_count"], 0, "{case_id}");
        assert_eq!(liminal["continuity_posture"], "RETRY_SIDE_EFFECT", "{case_id}");
    }

    if source["client_claim"] == "LOST"
        && source["external_outcome"] == "ONE_EFFECT_MATCHING"
    {
        assert_eq!(liminal["response_integrity"], "UNKNOWN", "{case_id}");
        assert_eq!(liminal["continuity_posture"], "REPORT_ONLY", "{case_id}");
    }

    if source["external_outcome"] == "MULTIPLE_EFFECTS_MATCHING" {
        assert_eq!(source["effect_count"], 2, "{case_id}");
        assert_eq!(liminal["observation_records"], 2, "{case_id}");
        assert_eq!(liminal["continuity_posture"], "BLOCKED", "{case_id}");
    }

    if source["external_outcome"] == "ONE_EFFECT_MISMATCHED" {
        assert_eq!(source["digests_match_admission"], false, "{case_id}");
        assert_eq!(liminal["response_integrity"], "FAILED", "{case_id}");
        assert_eq!(liminal["causal_validity"], "INVALID", "{case_id}");
        assert_eq!(liminal["continuity_posture"], "BLOCKED", "{case_id}");
    }
}

fn append_case(case: &Value) {
    source_contract(case);

    let case_id = case["case_id"].as_str().expect("case_id");
    let liminal = &case["liminal"];
    let expected = expected_dimensions(liminal);
    let observation_count = liminal["observation_records"]
        .as_u64()
        .expect("observation_records") as usize;
    let side_effect_committed = liminal["side_effect_committed"].as_bool();

    let root = tempdir().expect("tempdir");
    let final_projection = {
        let mut ledger = TrustworthyTransitionLedger::open(root.path()).expect("open ledger");

        let authorization = ledger
            .append(event(
                case_id,
                TransitionRecordKind::Authorization,
                "authorization",
                TransitionLinks::default(),
            ))
            .expect("authorization");

        let mut observations: Vec<TransitionEvent> = Vec::new();
        for index in 0..observation_count {
            let mut observation = event(
                case_id,
                TransitionRecordKind::Observation,
                &format!("observation-{index}"),
                TransitionLinks {
                    authorization_ref: Some(authorization.body.record_ref.clone()),
                    ..TransitionLinks::default()
                },
            );
            observation.dimensions = Some(TransitionDimensions {
                authority: AuthorityState::Valid,
                execution: expected.execution,
                response_integrity: ResponseIntegrityState::NotEvaluated,
                causal_validity: CausalValidityState::NotEvaluated,
                continuity_posture: ContinuityPosture::NotEvaluated,
            });
            observation.side_effect_committed = side_effect_committed;
            observations.push(ledger.append(observation).expect("observation"));
        }

        let observation_refs = observations
            .iter()
            .map(|item| item.body.record_ref.clone())
            .collect::<Vec<_>>();

        let mut response = event(
            case_id,
            TransitionRecordKind::ResponseIntegrity,
            "response-integrity",
            TransitionLinks {
                authorization_ref: Some(authorization.body.record_ref.clone()),
                observation_refs: observation_refs.clone(),
                ..TransitionLinks::default()
            },
        );
        response.dimensions = Some(TransitionDimensions {
            authority: AuthorityState::Valid,
            execution: expected.execution,
            response_integrity: expected.response_integrity,
            causal_validity: CausalValidityState::NotEvaluated,
            continuity_posture: ContinuityPosture::NotEvaluated,
        });
        response.side_effect_committed = side_effect_committed;
        let response = ledger.append(response).expect("response integrity");

        let mut causal_event = event(
            case_id,
            TransitionRecordKind::CausalAudit,
            "causal-audit",
            TransitionLinks {
                authorization_ref: Some(authorization.body.record_ref.clone()),
                observation_refs: observation_refs.clone(),
                response_integrity_ref: Some(response.body.record_ref.clone()),
                ..TransitionLinks::default()
            },
        );
        causal_event.dimensions = Some(TransitionDimensions {
            authority: AuthorityState::Valid,
            execution: expected.execution,
            response_integrity: expected.response_integrity,
            causal_validity: expected.causal_validity,
            continuity_posture: ContinuityPosture::NotEvaluated,
        });
        causal_event.side_effect_committed = side_effect_committed;
        let causal_event = ledger.append(causal_event).expect("causal audit");

        let mut continuity = event(
            case_id,
            TransitionRecordKind::ContinuitySnapshot,
            "continuity",
            TransitionLinks {
                authorization_ref: Some(authorization.body.record_ref),
                observation_refs,
                response_integrity_ref: Some(response.body.record_ref),
                causal_audit_ref: Some(causal_event.body.record_ref),
                previous_continuity_ref: None,
            },
        );
        continuity.dimensions = Some(expected.clone());
        continuity.side_effect_committed = side_effect_committed;
        ledger.append(continuity).expect("continuity");
        ledger.write_snapshot(2).expect("snapshot");

        ledger
            .projection(&format!("crashpoint:{case_id}"))
            .cloned()
            .expect("projection")
    };

    let reopened = TrustworthyTransitionLedger::open(root.path()).expect("reopen ledger");
    let recovered = reopened
        .projection(&format!("crashpoint:{case_id}"))
        .expect("recovered projection");

    assert_eq!(recovered, &final_projection, "{case_id}: replay drift");
    assert_eq!(
        recovered.dimensions.as_ref(),
        Some(&expected),
        "{case_id}: continuity dimensions"
    );
    assert_eq!(
        recovered.observation_refs.len(),
        observation_count,
        "{case_id}: observation multiplicity"
    );
    assert_eq!(
        recovered.side_effect_committed,
        side_effect_committed == Some(true),
        "{case_id}: committed-effect projection"
    );

    println!(
        "CRASHPOINT-LIMINALDB-001 case={case_id} posture={:?} execution={:?} observations={} side_effect_committed={}",
        expected.continuity_posture,
        expected.execution,
        observation_count,
        recovered.side_effect_committed
    );
}

#[test]
fn crashpoint_action_readback_maps_to_replayable_liminaldb_continuity() {
    let fixture: Value = serde_json::from_str(FIXTURE).expect("fixture JSON");
    assert_eq!(fixture["fixture_version"], "0.1");
    assert_eq!(fixture["system_case"], "CRASHPOINT-LIMINALDB-001");
    assert_eq!(fixture["source"]["trial_count"], 18);
    assert_eq!(fixture["source"]["trials_per_case"], 3);

    let cases = fixture["cases"].as_array().expect("cases");
    assert_eq!(cases.len(), 6);

    for case in cases {
        append_case(case);
    }
}

#[test]
fn unavailable_readback_never_becomes_no_effect_or_retry_permission() {
    let fixture: Value = serde_json::from_str(FIXTURE).expect("fixture JSON");
    let case = fixture["cases"]
        .as_array()
        .expect("cases")
        .iter()
        .find(|case| case["case_id"] == "unavailable_readback")
        .expect("unavailable_readback case");

    assert_eq!(case["source"]["external_outcome"], "INDETERMINATE");
    assert_eq!(case["source"]["observation_availability"], "UNAVAILABLE");
    assert_eq!(case["source"]["externally_verified"], false);
    assert!(case["source"]["effect_count"].is_null());
    assert_eq!(case["liminal"]["continuity_posture"], "REVALIDATE");
    assert!(case["liminal"]["side_effect_committed"].is_null());
}
