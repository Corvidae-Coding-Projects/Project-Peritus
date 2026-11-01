//! Independent H0 readiness oracle over the public security-policy evaluator.
//! All candidate identities and evidence are synthetic.

use peritus_security_policy::{
    AcceptanceCriterion, ArtifactObservation, CriterionObservation, EvidenceArtifactKind,
    FindingLifecycle, FindingObservation, FindingSeverity, IndependentSecurityReview,
    IntegratedCandidate, InventoryKind, InventoryObservation, ObservationClass,
    RequirementObservation, ReviewCompletion, ReviewScope, ReviewerIdentity,
    SecurityControlOutcome, SecurityEvidence, SecurityRequirement, UnmetSecurityCondition,
    evaluate_security_readiness,
};
use peritus_types::{
    AcceptanceSpecId, ActorId, FindingId, Generation, HarnessId, PolicyId, ProviderProfileId,
    RevisionNumber, RevisionTuple, Sha256Digest, WorkspaceId,
};
use proptest::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ObservationFault {
    Complete,
    Missing,
    Failed,
    NotExecuted,
    EmptyDigest,
    Stale,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum InventoryFault {
    Complete,
    Missing,
    Incomplete,
    EmptyDigest,
    Stale,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ArtifactFault {
    Complete,
    Missing,
    EmptyDigest,
    Stale,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReviewFault {
    Complete,
    Missing,
    Incomplete,
    SameActor,
    SameOrganization,
    EmptyReport,
    MissingScope,
    Stale,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum FindingFault {
    None,
    HighOpen,
    HighAcceptedRisk,
    HighResolved,
    MediumOpen,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Case {
    pub requirement_index: u8,
    pub requirement: ObservationFault,
    pub criterion_index: u8,
    pub criterion: ObservationFault,
    pub inventory_index: u8,
    pub inventory: InventoryFault,
    pub artifact_index: u8,
    pub artifact: ArtifactFault,
    pub review: ReviewFault,
    pub finding: FindingFault,
}

fn clean() -> Case {
    Case {
        requirement_index: 0,
        requirement: ObservationFault::Complete,
        criterion_index: 0,
        criterion: ObservationFault::Complete,
        inventory_index: 0,
        inventory: InventoryFault::Complete,
        artifact_index: 0,
        artifact: ArtifactFault::Complete,
        review: ReviewFault::Complete,
        finding: FindingFault::None,
    }
}

pub fn examples() -> Vec<Case> {
    let mut cases = vec![clean()];
    let mut case = clean();
    case.requirement = ObservationFault::Stale;
    case.criterion = ObservationFault::Failed;
    case.inventory = InventoryFault::EmptyDigest;
    case.artifact = ArtifactFault::Missing;
    case.review = ReviewFault::SameActor;
    case.finding = FindingFault::HighOpen;
    cases.push(case);
    let mut case = clean();
    case.review = ReviewFault::Missing;
    cases.push(case);
    cases
}

pub fn calibration() -> Case {
    examples().remove(1)
}

pub fn strategy(_max_steps: usize) -> BoxedStrategy<Case> {
    let observation = prop::sample::select(vec![
        ObservationFault::Complete,
        ObservationFault::Missing,
        ObservationFault::Failed,
        ObservationFault::NotExecuted,
        ObservationFault::EmptyDigest,
        ObservationFault::Stale,
    ]);
    let inventory = prop::sample::select(vec![
        InventoryFault::Complete,
        InventoryFault::Missing,
        InventoryFault::Incomplete,
        InventoryFault::EmptyDigest,
        InventoryFault::Stale,
    ]);
    let artifact = prop::sample::select(vec![
        ArtifactFault::Complete,
        ArtifactFault::Missing,
        ArtifactFault::EmptyDigest,
        ArtifactFault::Stale,
    ]);
    let review = prop::sample::select(vec![
        ReviewFault::Complete,
        ReviewFault::Missing,
        ReviewFault::Incomplete,
        ReviewFault::SameActor,
        ReviewFault::SameOrganization,
        ReviewFault::EmptyReport,
        ReviewFault::MissingScope,
        ReviewFault::Stale,
    ]);
    let finding = prop::sample::select(vec![
        FindingFault::None,
        FindingFault::HighOpen,
        FindingFault::HighAcceptedRisk,
        FindingFault::HighResolved,
        FindingFault::MediumOpen,
    ]);
    (
        (0_u8..7, observation.clone()),
        (0_u8..9, observation),
        (0_u8..4, inventory),
        (0_u8..10, artifact),
        review,
        finding,
    )
        .prop_map(|(req, criterion, inventory, artifact, review, finding)| Case {
            requirement_index: req.0,
            requirement: req.1,
            criterion_index: criterion.0,
            criterion: criterion.1,
            inventory_index: inventory.0,
            inventory: inventory.1,
            artifact_index: artifact.0,
            artifact: artifact.1,
            review,
            finding,
        })
        .boxed()
}

fn id(tag: u8) -> [u8; 16] {
    [tag; 16]
}

fn digest(tag: u8) -> Sha256Digest {
    Sha256Digest::new([tag; 32])
}

fn revision() -> RevisionTuple {
    RevisionTuple::new(
        AcceptanceSpecId::new(id(1)).expect("synthetic id"),
        HarnessId::new(id(2)).expect("synthetic id"),
        WorkspaceId::new(id(3)).expect("synthetic id"),
        Generation::new(1).expect("nonzero generation"),
        RevisionNumber::new(1).expect("nonzero revision"),
        PolicyId::new(id(4)).expect("synthetic id"),
        ProviderProfileId::new(id(5)).expect("synthetic id"),
    )
}

fn candidates() -> (IntegratedCandidate, IntegratedCandidate) {
    let revision = revision();
    (
        IntegratedCandidate::new(revision, digest(11), digest(12), digest(13)),
        IntegratedCandidate::new(revision, digest(21), digest(12), digest(13)),
    )
}

fn observed_candidate(
    candidate: IntegratedCandidate,
    stale: IntegratedCandidate,
    fault: bool,
) -> IntegratedCandidate {
    if fault { stale } else { candidate }
}

fn control_outcome(fault: ObservationFault) -> SecurityControlOutcome {
    match fault {
        ObservationFault::Failed => SecurityControlOutcome::Failed,
        ObservationFault::NotExecuted => SecurityControlOutcome::NotExecuted,
        _ => SecurityControlOutcome::Passed,
    }
}

fn build_evidence(
    case: &Case,
    candidate: IntegratedCandidate,
    stale: IntegratedCandidate,
) -> Result<SecurityEvidence, String> {
    let mut requirements = Vec::new();
    for (index, requirement) in SecurityRequirement::ALL.into_iter().enumerate() {
        let fault = if index == usize::from(case.requirement_index) {
            case.requirement
        } else {
            ObservationFault::Complete
        };
        if fault == ObservationFault::Missing {
            continue;
        }
        requirements.push(RequirementObservation::new(
            requirement,
            observed_candidate(candidate, stale, fault == ObservationFault::Stale),
            control_outcome(fault),
            if fault == ObservationFault::EmptyDigest { digest(0) } else { digest(31) },
        ));
    }
    let mut criteria = Vec::new();
    for (index, criterion) in AcceptanceCriterion::ALL.into_iter().enumerate() {
        let fault = if index == usize::from(case.criterion_index) {
            case.criterion
        } else {
            ObservationFault::Complete
        };
        if fault == ObservationFault::Missing {
            continue;
        }
        criteria.push(CriterionObservation::new(
            criterion,
            observed_candidate(candidate, stale, fault == ObservationFault::Stale),
            control_outcome(fault),
            if fault == ObservationFault::EmptyDigest { digest(0) } else { digest(32) },
        ));
    }
    let mut inventories = Vec::new();
    for (index, kind) in InventoryKind::ALL.into_iter().enumerate() {
        let fault = if index == usize::from(case.inventory_index) {
            case.inventory
        } else {
            InventoryFault::Complete
        };
        if fault == InventoryFault::Missing {
            continue;
        }
        inventories.push(InventoryObservation::new(
            kind,
            observed_candidate(candidate, stale, fault == InventoryFault::Stale),
            fault != InventoryFault::Incomplete,
            if fault == InventoryFault::EmptyDigest { digest(0) } else { digest(33) },
        ));
    }
    let mut artifacts = Vec::new();
    for (index, kind) in EvidenceArtifactKind::ALL.into_iter().enumerate() {
        let fault = if index == usize::from(case.artifact_index) {
            case.artifact
        } else {
            ArtifactFault::Complete
        };
        if fault == ArtifactFault::Missing {
            continue;
        }
        artifacts.push(ArtifactObservation::new(
            kind,
            observed_candidate(candidate, stale, fault == ArtifactFault::Stale),
            if fault == ArtifactFault::EmptyDigest { digest(0) } else { digest(34) },
        ));
    }
    let review = if case.review == ReviewFault::Missing {
        None
    } else {
        let producer = ActorId::new(id(41)).expect("synthetic actor");
        let reviewer = ReviewerIdentity::new(
            if case.review == ReviewFault::SameActor {
                producer
            } else {
                ActorId::new(id(42)).expect("synthetic actor")
            },
            if case.review == ReviewFault::SameOrganization { digest(43) } else { digest(44) },
            digest(45),
        );
        let mut scopes = ReviewScope::ALL.to_vec();
        if case.review == ReviewFault::MissingScope {
            scopes.remove(2);
        }
        let findings = match case.finding {
            FindingFault::None => vec![],
            finding => {
                let severity = if finding == FindingFault::MediumOpen {
                    FindingSeverity::Medium
                } else {
                    FindingSeverity::High
                };
                let lifecycle = match finding {
                    FindingFault::HighResolved => FindingLifecycle::Resolved {
                        remediation_digest: digest(46),
                        retest_digest: digest(47),
                    },
                    FindingFault::HighAcceptedRisk => {
                        FindingLifecycle::AcceptedRisk { authority_digest: digest(48) }
                    }
                    _ => FindingLifecycle::Open,
                };
                vec![FindingObservation::new(
                    FindingId::new(id(49)).expect("synthetic finding"),
                    observed_candidate(candidate, stale, case.review == ReviewFault::Stale),
                    severity,
                    lifecycle,
                )]
            }
        };
        Some(
            IndependentSecurityReview::new(
                observed_candidate(candidate, stale, case.review == ReviewFault::Stale),
                reviewer,
                producer,
                digest(43),
                if case.review == ReviewFault::Incomplete {
                    ReviewCompletion::Incomplete
                } else {
                    ReviewCompletion::Completed
                },
                scopes,
                if case.review == ReviewFault::EmptyReport { digest(0) } else { digest(50) },
                findings,
            )
            .map_err(|error| format!("security review fixture: {error:?}"))?,
        )
    };
    SecurityEvidence::new(requirements, criteria, inventories, artifacts, review)
        .map_err(|error| format!("security evidence fixture: {error:?}"))
}

fn expected(case: &Case) -> Vec<UnmetSecurityCondition> {
    use UnmetSecurityCondition as Unmet;
    let mut unmet = Vec::new();
    if case.requirement == ObservationFault::Stale {
        unmet.push(Unmet::CandidateMismatch {
            class: ObservationClass::Requirement,
            index: usize::from(case.requirement_index),
        });
    }
    if case.criterion == ObservationFault::Stale {
        unmet.push(Unmet::CandidateMismatch {
            class: ObservationClass::Criterion,
            index: usize::from(case.criterion_index),
        });
    }
    if case.inventory == InventoryFault::Stale {
        unmet.push(Unmet::CandidateMismatch {
            class: ObservationClass::Inventory,
            index: usize::from(case.inventory_index),
        });
    }
    if case.artifact == ArtifactFault::Stale {
        unmet.push(Unmet::CandidateMismatch {
            class: ObservationClass::Artifact,
            index: usize::from(case.artifact_index),
        });
    }
    if case.review == ReviewFault::Stale {
        unmet.push(Unmet::CandidateMismatch { class: ObservationClass::ExternalReview, index: 0 });
        if case.finding != FindingFault::None {
            unmet.push(Unmet::CandidateMismatch { class: ObservationClass::Finding, index: 0 });
        }
    }
    let requirement = SecurityRequirement::ALL[usize::from(case.requirement_index)];
    match case.requirement {
        ObservationFault::Missing | ObservationFault::Stale => {
            unmet.push(Unmet::MissingRequirement(requirement))
        }
        ObservationFault::Failed | ObservationFault::NotExecuted => {
            unmet.push(Unmet::RequirementDidNotPass {
                requirement,
                outcome: control_outcome(case.requirement),
            })
        }
        ObservationFault::EmptyDigest => unmet.push(Unmet::EmptyRequirementEvidence(requirement)),
        ObservationFault::Complete => {}
    }
    let criterion = AcceptanceCriterion::ALL[usize::from(case.criterion_index)];
    match case.criterion {
        ObservationFault::Missing | ObservationFault::Stale => {
            unmet.push(Unmet::MissingCriterion(criterion))
        }
        ObservationFault::Failed | ObservationFault::NotExecuted => {
            unmet.push(Unmet::CriterionDidNotPass {
                criterion,
                outcome: control_outcome(case.criterion),
            })
        }
        ObservationFault::EmptyDigest => unmet.push(Unmet::EmptyCriterionEvidence(criterion)),
        ObservationFault::Complete => {}
    }
    let inventory = InventoryKind::ALL[usize::from(case.inventory_index)];
    match case.inventory {
        InventoryFault::Missing | InventoryFault::Stale => {
            unmet.push(Unmet::MissingInventory(inventory))
        }
        InventoryFault::Incomplete => unmet.push(Unmet::InventoryIncomplete(inventory)),
        InventoryFault::EmptyDigest => unmet.push(Unmet::EmptyInventoryDigest(inventory)),
        InventoryFault::Complete => {}
    }
    match case.review {
        ReviewFault::Missing => unmet.push(Unmet::MissingExternalReview),
        ReviewFault::Incomplete => unmet.push(Unmet::ExternalReviewIncomplete),
        ReviewFault::SameActor | ReviewFault::SameOrganization => {
            unmet.push(Unmet::ExternalReviewNotIndependent)
        }
        ReviewFault::EmptyReport => unmet.push(Unmet::EmptyExternalReviewDigest),
        ReviewFault::MissingScope => {
            unmet.push(Unmet::MissingExternalReviewScope(ReviewScope::ALL[2]))
        }
        ReviewFault::Complete | ReviewFault::Stale => {}
    }
    if case.review != ReviewFault::Missing
        && matches!(case.finding, FindingFault::HighOpen | FindingFault::HighAcceptedRisk)
    {
        unmet.push(Unmet::UnresolvedReleaseBlocker {
            finding_id: FindingId::new(id(49)).expect("synthetic finding"),
            severity: FindingSeverity::High,
        });
    }
    let artifact = EvidenceArtifactKind::ALL[usize::from(case.artifact_index)];
    match case.artifact {
        ArtifactFault::Missing | ArtifactFault::Stale => {
            unmet.push(Unmet::MissingEvidenceArtifact(artifact))
        }
        ArtifactFault::EmptyDigest => unmet.push(Unmet::EmptyEvidenceDigest(artifact)),
        ArtifactFault::Complete => {}
    }
    unmet
}

fn bump(metrics: &mut crate::Metrics, key: &str) {
    *metrics.entry(key.to_owned()).or_default() += 1;
}

pub fn check(case: &Case) -> Result<crate::Metrics, String> {
    if case.requirement_index >= 7
        || case.criterion_index >= 9
        || case.inventory_index >= 4
        || case.artifact_index >= 10
    {
        return Err("setup-incomplete: security catalog index bound".into());
    }
    let (candidate, stale) = candidates();
    let evidence = build_evidence(case, candidate, stale)?;
    let decision = evaluate_security_readiness(candidate, &evidence);
    let wanted = expected(case);
    if decision.unmet_conditions() != wanted || decision.is_ready() != wanted.is_empty() {
        return Err(format!(
            "security readiness mismatch: case={case:?} expected={wanted:?} actual={decision:?}"
        ));
    }
    let mut metrics = crate::Metrics::new();
    bump(&mut metrics, "security_cases");
    bump(&mut metrics, &format!("security_requirement_{:?}", case.requirement));
    bump(&mut metrics, &format!("security_criterion_{:?}", case.criterion));
    bump(&mut metrics, &format!("security_inventory_{:?}", case.inventory));
    bump(&mut metrics, &format!("security_artifact_{:?}", case.artifact));
    bump(&mut metrics, &format!("security_review_{:?}", case.review));
    bump(&mut metrics, &format!("security_finding_{:?}", case.finding));
    Ok(metrics)
}
