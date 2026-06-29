use std::collections::BTreeSet;

use serde_json::Value;

const FIXTURE: &str = include_str!("fixtures/trustworthy_transition_ledger_v0.1.json");

#[test]
fn trustworthy_transition_fixture_is_versioned_and_unambiguous() {
    let fixture: Value = serde_json::from_str(FIXTURE).expect("fixture must be valid JSON");
    assert_eq!(fixture["fixture_version"], "0.1");
    assert_eq!(
        fixture["profile"],
        "org.liminaldb.trustworthy-transition-ledger.v0.1"
    );

    let cases = fixture["cases"].as_array().expect("cases array");
    assert_eq!(cases.len(), 10);

    let expected_errors: BTreeSet<&str> = [
        "PARENT_MISMATCH",
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

        let has_expected = case.get("expected").is_some();
        let has_error = case.get("expected_error").is_some();
        assert_ne!(
            has_expected, has_error,
            "case {case_id} must define exactly one expected outcome"
        );

        if let Some(error) = case.get("expected_error") {
            let error = error.as_str().expect("expected_error must be a string");
            assert!(
                expected_errors.contains(error),
                "case {case_id} uses an unknown stable error code: {error}"
            );
        }
    }
}
