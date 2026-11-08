//! Replays of scheduling, evidence, and durable-state failure boundaries.

use super::{
    contract::{CapabilityKind, CapabilityReview},
    *,
};
use crate::developer_tools::{CommandPurpose, SuccessfulCommand};

fn open(directory: &std::path::Path, source: &str) -> Delivery {
    Delivery::open(
        directory.join("delivery.json"),
        "run".into(),
        "workspace".into(),
        "foundation consumer independent".into(),
        source.into(),
        vec!["r1".into(), "r2".into()],
    )
    .unwrap()
}

fn plan() -> Vec<Capability> {
    ["foundation", "consumer", "independent"]
        .into_iter()
        .map(|id| Capability {
            id: id.into(),
            requirements: vec![if id == "consumer" { "r2" } else { "r1" }.into()],
            source_quote: id.into(),
            outcome: format!("real {id} operation"),
            depends_on: if id == "consumer" { vec!["foundation".into()] } else { Vec::new() },
            kind: CapabilityKind::Behavior,
            positive: "real positive output".into(),
            negative: "rejection preserves state".into(),
        })
        .collect()
}

fn approved(directory: &std::path::Path) -> Delivery {
    let mut delivery = open(directory, "scope-1");
    delivery.propose(plan()).unwrap();
    delivery
        .review_scope(
            &DeliveryReview {
                scope_complete: true,
                scope_evidence: "independent complete scope mapping".into(),
                capabilities: Vec::new(),
            },
            false,
        )
        .unwrap();
    delivery
}

fn handoff(id: &str, remaining: Vec<String>) -> DraftHandoff {
    DraftHandoff {
        capability: id.into(),
        implementation: format!("src/{id}.rs connected to caller"),
        remaining,
    }
}

fn review() -> DeliveryReview {
    DeliveryReview {
        scope_complete: true,
        scope_evidence: "each requested operation and failure is mapped".into(),
        capabilities: plan()
            .iter()
            .map(|item| CapabilityReview {
                id: item.id.clone(),
                satisfied: true,
                positive_observation: "native public operation returned expected data".into(),
                negative_observation: "rejected input preserved the independent state".into(),
                commands: vec![command("candidate").verification_reference()],
            })
            .collect(),
    }
}

fn command(candidate: &str) -> SuccessfulCommand {
    SuccessfulCommand {
        command: "run_command real-check".into(),
        purpose: CommandPurpose::Verification,
        observed_candidate: Some(candidate.into()),
    }
}

fn drafted(directory: &std::path::Path) -> Delivery {
    let mut delivery = approved(directory);
    for id in ["foundation", "consumer", "independent"] {
        delivery.draft(handoff(id, Vec::new())).unwrap();
    }
    delivery
}

#[test]
fn dependency_order_segment_rotation_and_restart_preserve_unfinished_work() {
    let directory = tempfile::tempdir().unwrap();
    let mut delivery = approved(directory.path());
    assert_eq!(delivery.active().unwrap().id, "foundation");
    assert!(delivery.draft(handoff("consumer", Vec::new())).is_err());
    delivery.rotate().unwrap();
    assert_eq!(delivery.active().unwrap().id, "independent");
    delivery.draft(handoff("independent", vec!["OPEN handler still unsupported".into()])).unwrap();
    drop(delivery);
    let mut delivery = open(directory.path(), "scope-1");
    assert!(delivery.prompt().unwrap().contains("OPEN handler still unsupported"));
    assert_eq!(delivery.active().unwrap().id, "foundation");
    delivery.draft(handoff("foundation", Vec::new())).unwrap();
    assert_eq!(delivery.active().unwrap().id, "consumer");
    delivery.draft(handoff("consumer", Vec::new())).unwrap();
    assert_eq!(delivery.active().unwrap().id, "independent");
    assert!(delivery.check_review(&review(), false, "candidate", &[command("candidate")]).is_err());
    delivery.draft(handoff("independent", Vec::new())).unwrap();
    assert!(!delivery.constructing());
    assert!(!delivery.is_qualified("candidate"));
    let reviewer = delivery.review_prompt(false).unwrap();
    assert!(
        !reviewer.contains("During CONSTRUCTION"),
        "reviewers must not receive developer terminal instructions"
    );
    assert!(reviewer.contains("Append `delivery` to the normal review JSON"));
}

#[test]
fn missing_duplicate_fabricated_and_stale_capability_proofs_cannot_qualify() {
    let directory = tempfile::tempdir().unwrap();
    let mut delivery = drafted(directory.path());
    assert!(delivery.require_verification_handoff("candidate", &[]).is_err());
    assert!(
        delivery.require_verification_handoff("candidate", &[command("before-cleanup")]).is_err()
    );
    assert!(delivery.require_verification_handoff("candidate", &[command("candidate")]).is_ok());
    let mut missing = review();
    missing.capabilities.pop();
    assert!(delivery.qualify(missing, "candidate", &[command("candidate")]).is_err());
    let mut duplicate = review();
    duplicate.capabilities[1] = duplicate.capabilities[0].clone();
    assert!(delivery.qualify(duplicate, "candidate", &[command("candidate")]).is_err());
    assert!(delivery.qualify(review(), "candidate", &[]).is_err());
    assert!(delivery.qualify(review(), "candidate", &[command("old-source")]).is_err());
    assert!(delivery.qualify(review(), "candidate", &[command("candidate")]).unwrap());
    assert!(open(directory.path(), "scope-1").is_qualified("candidate"));
    assert!(!open(directory.path(), "scope-1").is_qualified("changed-source"));
    let mut new_input = open(directory.path(), "scope-1");
    new_input.revision = 2;
    assert!(!new_input.is_qualified("candidate"));
    delivery.state.qualification.as_mut().unwrap().1.capabilities.remove(0);
    delivery.save().unwrap();
    assert!(!open(directory.path(), "scope-1").is_qualified("candidate"));
}

#[test]
fn unsupported_behavior_does_not_inherit_another_capabilitys_green_verification() {
    let directory = tempfile::tempdir().unwrap();
    let mut delivery = drafted(directory.path());
    let mut assessment = review();
    assessment.capabilities[1].satisfied = false;
    assessment.capabilities[1].positive_observation =
        "consumer returns Unsupported for every valid input".into();
    assessment.capabilities[1].commands.clear();
    assert!(!delivery.qualify(assessment, "candidate", &[command("candidate")]).unwrap());
    assert!(delivery.requirement_evidence("r1", "candidate").0);
    assert!(!delivery.requirement_evidence("r2", "candidate").0);
    assert!(!delivery.is_qualified("candidate"));
    assert!(delivery.prompt().unwrap().contains("consumer returns Unsupported"));
}

#[test]
fn scope_changes_invalidate_progress_without_destroying_the_previous_contract() {
    let directory = tempfile::tempdir().unwrap();
    let delivery = drafted(directory.path());
    let mut weakened = plan();
    weakened.pop();
    let mut delivery = delivery;
    assert!(delivery.propose(weakened).is_err());
    let rebound = open(directory.path(), "scope-2");
    assert!(rebound.planning());
    assert!(!rebound.is_qualified("candidate"));
    assert!(rebound.prompt().unwrap().contains("real consumer operation"));
    assert!(
        Delivery::open(
            directory.path().join("delivery.json"),
            "other-run".into(),
            "workspace".into(),
            "foundation consumer independent".into(),
            "scope-1".into(),
            vec!["r1".into(), "r2".into()]
        )
        .is_err()
    );
}

#[test]
fn incomplete_scope_missing_requirements_and_dependency_cycles_are_rejected() {
    let directory = tempfile::tempdir().unwrap();
    let mut delivery = open(directory.path(), "scope-1");
    let mut omitted = plan();
    omitted.remove(1);
    assert!(delivery.propose(omitted).is_err());
    let mut cycle = plan();
    cycle[0].depends_on.push("consumer".into());
    assert!(delivery.propose(cycle).is_err());
    delivery.propose(plan()).unwrap();
    delivery
        .review_scope(
            &DeliveryReview {
                scope_complete: false,
                scope_evidence: "missing subsystem".into(),
                capabilities: Vec::new(),
            },
            false,
        )
        .unwrap();
    assert!(delivery.planning());
    assert!(delivery.prompt().unwrap().contains("missing subsystem"));
    let path = directory.path().join("delivery.json");
    let mut broken: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    broken["cursor"] = serde_json::json!(u64::MAX);
    std::fs::write(&path, serde_json::to_vec(&broken).unwrap()).unwrap();
    assert!(
        Delivery::open(
            path,
            "run".into(),
            "workspace".into(),
            "foundation consumer independent".into(),
            "scope-1".into(),
            vec!["r1".into(), "r2".into()]
        )
        .is_err()
    );
}

#[cfg(unix)]
#[test]
fn a_symlink_cannot_supply_host_owned_delivery_state() {
    let directory = tempfile::tempdir().unwrap();
    std::os::unix::fs::symlink("/dev/null", directory.path().join("delivery.json")).unwrap();
    assert!(
        Delivery::open(
            directory.path().join("delivery.json"),
            "run".into(),
            "workspace".into(),
            "scope".into(),
            "digest".into(),
            vec!["r1".into()]
        )
        .is_err()
    );
}

#[test]
fn repeated_segments_force_smaller_work_and_rejected_replans_cannot_drop_scope() {
    let directory = tempfile::tempdir().unwrap();
    let mut delivery = approved(directory.path());
    delivery.draft(handoff("foundation", Vec::new())).unwrap();
    delivery.draft(handoff("consumer", Vec::new())).unwrap();
    for _ in 0..3 {
        delivery.segment_boundary().unwrap();
    }
    assert!(open(directory.path(), "scope-1").planning());
    assert!(delivery.propose(plan()).is_err());
    let mut smaller = plan();
    let mut child = smaller[2].clone();
    child.id = "independent-minimum".into();
    smaller[2].depends_on.push(child.id.clone());
    smaller.push(child);
    delivery.propose(smaller).unwrap();
    delivery
        .review_scope(
            &DeliveryReview {
                scope_complete: false,
                scope_evidence: "make the child more concrete".into(),
                capabilities: Vec::new(),
            },
            false,
        )
        .unwrap();
    let mut dropped = plan();
    dropped.remove(2);
    assert!(delivery.propose(dropped).is_err());
    let mut smaller = plan();
    let mut child = smaller[2].clone();
    child.id = "independent-minimum".into();
    smaller[2].depends_on.push(child.id.clone());
    smaller.push(child);
    delivery.propose(smaller).unwrap();
    delivery
        .review_scope(
            &DeliveryReview {
                scope_complete: true,
                scope_evidence: "original outcomes preserved, smaller prerequisite admitted".into(),
                capabilities: Vec::new(),
            },
            false,
        )
        .unwrap();
    assert_eq!(delivery.active().unwrap().id, "independent-minimum");
    assert_eq!(delivery.state.drafted.len(), 2);
    delivery.draft(handoff("independent-minimum", Vec::new())).unwrap();
    assert_eq!(delivery.active().unwrap().id, "independent");
}
