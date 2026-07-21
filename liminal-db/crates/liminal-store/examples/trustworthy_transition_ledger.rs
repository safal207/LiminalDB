use std::fs;

use liminal_store::{
    sha256_ref, AuthorityState, CausalValidityState, ContinuityPosture, ExecutionState,
    ResponseIntegrityState, TransitionDimensions, TransitionEventInput, TransitionLinks,
    TransitionRecordKind, TrustworthyTransitionLedger,
};

fn reference(label: &str) -> String {
    sha256_ref(label.as_bytes())
}

fn dimensions(
    execution: ExecutionState,
    integrity: ResponseIntegrityState,
    causal: CausalValidityState,
    posture: ContinuityPosture,
) -> TransitionDimensions {
    TransitionDimensions {
        authority: AuthorityState::Valid,
        execution,
        response_integrity: integrity,
        causal_validity: causal,
        continuity_posture: posture,
    }
}

fn event(kind: TransitionRecordKind, label: &str, links: TransitionLinks) -> TransitionEventInput {
    TransitionEventInput {
        transition_id: "demo-transition-001".into(),
        subject_id: "agent:reporter".into(),
        kind,
        record_ref: reference(&format!("record:{label}")),
        payload_digest: reference(&format!("payload:{label}")),
        links,
        dimensions: None,
        side_effect_committed: None,
        captured_at_ms: 1,
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = std::env::temp_dir().join(format!(
        "liminaldb-trustworthy-transition-demo-{}",
        std::process::id()
    ));
    if root.exists() {
        fs::remove_dir_all(&root)?;
    }

    let final_projection = {
        let mut ledger = TrustworthyTransitionLedger::open(&root)?;
        let authorization = ledger.append(event(
            TransitionRecordKind::Authorization,
            "authorization",
            TransitionLinks::default(),
        ))?;

        let mut observation = event(
            TransitionRecordKind::Observation,
            "observation",
            TransitionLinks {
                authorization_ref: Some(authorization.body.record_ref.clone()),
                ..TransitionLinks::default()
            },
        );
        observation.dimensions = Some(dimensions(
            ExecutionState::ObservedExecuted,
            ResponseIntegrityState::NotEvaluated,
            CausalValidityState::NotEvaluated,
            ContinuityPosture::NotEvaluated,
        ));
        observation.side_effect_committed = Some(true);
        let observation = ledger.append(observation)?;

        let mut integrity = event(
            TransitionRecordKind::ResponseIntegrity,
            "response-integrity",
            TransitionLinks {
                authorization_ref: Some(authorization.body.record_ref.clone()),
                observation_refs: vec![observation.body.record_ref.clone()],
                ..TransitionLinks::default()
            },
        );
        integrity.dimensions = Some(dimensions(
            ExecutionState::ObservedExecuted,
            ResponseIntegrityState::Failed,
            CausalValidityState::NotEvaluated,
            ContinuityPosture::RemediateResponse,
        ));
        let integrity = ledger.append(integrity)?;

        let mut causal = event(
            TransitionRecordKind::CausalAudit,
            "causal-audit",
            TransitionLinks {
                authorization_ref: Some(authorization.body.record_ref.clone()),
                observation_refs: vec![observation.body.record_ref.clone()],
                response_integrity_ref: Some(integrity.body.record_ref.clone()),
                ..TransitionLinks::default()
            },
        );
        causal.dimensions = Some(dimensions(
            ExecutionState::ObservedExecuted,
            ResponseIntegrityState::Failed,
            CausalValidityState::Valid,
            ContinuityPosture::RemediateResponse,
        ));
        let causal = ledger.append(causal)?;

        let mut continuity = event(
            TransitionRecordKind::ContinuitySnapshot,
            "continuity-snapshot",
            TransitionLinks {
                authorization_ref: Some(authorization.body.record_ref),
                observation_refs: vec![observation.body.record_ref],
                response_integrity_ref: Some(integrity.body.record_ref),
                causal_audit_ref: Some(causal.body.record_ref),
                previous_continuity_ref: None,
            },
        );
        continuity.dimensions = Some(dimensions(
            ExecutionState::ObservedExecuted,
            ResponseIntegrityState::Failed,
            CausalValidityState::Valid,
            ContinuityPosture::RemediateResponse,
        ));
        continuity.side_effect_committed = Some(true);
        ledger.append(continuity)?;
        ledger.write_snapshot(2)?;
        ledger
            .projection("demo-transition-001")
            .cloned()
            .expect("projection exists")
    };

    let reopened = TrustworthyTransitionLedger::open(&root)?;
    let recovered = reopened
        .projection("demo-transition-001")
        .expect("recovered projection");
    assert_eq!(recovered, &final_projection);

    println!("Recovered {} durable events", reopened.event_count());
    println!("Projection: {recovered:#?}");
    drop(reopened);

    fs::remove_dir_all(root)?;
    Ok(())
}
