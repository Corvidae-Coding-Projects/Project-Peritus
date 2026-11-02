//! Signed approval histories; the expected lifecycle and binding checks are independent data.

use ed25519_dalek::{Signer, SigningKey};
use peritus_approval::{
    ActionDigest, AmendmentIdentity, ApprovalAggregate, ApprovalChoice, ApprovalDecision,
    ApprovalKeyId, ApprovalPhase, ApprovalPublicKey, ApprovalRequest, ApprovalSignature,
    ApproverCredential, CredentialRegistrySnapshot, CredentialStatus, ParticipantSet,
    SignedApprovalDecision, decode_approval_request, decode_signed_decision,
    encode_approval_request, encode_signed_decision, verify_signed_decision,
};
use peritus_policy::{
    ActorRole, ActorSelector, ApprovalRequirement, AuthorityBoundary, AuthorityCeiling,
    AuthorityInstant, AuthorityTier, AuthorityTimeState, AuthorizationRequest, CapabilityScope,
    CeilingGrant, EnvironmentSelector, IndependenceRequirement, IndependenceSet, OperationClass,
    OperationDescriptor, OperationRegistry, Permission, PermissionSelector, PermissionSet,
    PolicyDefinition, PolicyTier, RestrictionLayer, RestrictionRule, RiskClass, RiskSet,
    RoleSelector, ScopeSelector, UseLimit, ValidityWindow,
};
use peritus_types::{
    AcceptanceSpecId, ActionId, ActorId, ApprovalRequestId, CapabilityName, CommandId,
    EnvironmentId, Generation, HarnessId, PolicyId, ProviderProfileId, ResourceId, RevisionNumber,
    RevisionTuple, Sha256Digest, WorkspaceId,
};
use proptest::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Defect {
    None,
    Signature,
    WrongSigner,
    RequestId,
    RequestDigest,
    RegistryRevision,
    Generation,
    Disabled,
    Environment,
    Workspace,
    Role,
    Tier,
    Actor,
    Action,
    ActionDigest,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Approve,
    Amend,
    Deny,
    Consume,
    Cancel,
    Expire,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Operation {
    pub kind: Kind,
    pub defect: Defect,
    pub time: u8,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Case {
    pub request_expiry: u8,
    pub credential_expiry: u8,
    pub decision_expiry: u8,
    pub independence: u8,
    pub responder: u8,
    pub unlimited_scope: bool,
    pub all_approver_roles: bool,
    pub all_risks: bool,
    pub operations: Vec<Operation>,
}

fn base() -> Case {
    Case {
        request_expiry: 70,
        credential_expiry: 80,
        decision_expiry: 60,
        independence: 15,
        responder: 5,
        unlimited_scope: false,
        all_approver_roles: false,
        all_risks: false,
        operations: Vec::new(),
    }
}
pub fn strategy(max_steps: usize) -> BoxedStrategy<Case> {
    let op=(prop_oneof![Just(Kind::Approve),Just(Kind::Amend),Just(Kind::Deny),Just(Kind::Consume),Just(Kind::Cancel),Just(Kind::Expire)],
        prop_oneof![4=>Just(Defect::None),1=>Just(Defect::Signature),1=>Just(Defect::WrongSigner),
        1=>Just(Defect::RequestId),1=>Just(Defect::RequestDigest),1=>Just(Defect::RegistryRevision),
        1=>Just(Defect::Generation),1=>Just(Defect::Disabled),1=>Just(Defect::Environment),
        1=>Just(Defect::Workspace),1=>Just(Defect::Role),1=>Just(Defect::Tier),1=>Just(Defect::Actor),
        1=>Just(Defect::Action),1=>Just(Defect::ActionDigest)],0_u8..=9)
        .prop_map(|(kind,defect,time)|Operation{kind,defect,time});
    let general = (
        21_u8..=110,
        21_u8..=110,
        21_u8..=110,
        0_u8..=15,
        1_u8..=5,
        any::<bool>(),
        any::<bool>(),
        any::<bool>(),
        proptest::collection::vec(op.clone(), 0..=max_steps.min(128)),
    )
        .prop_map(
            |(
                request_expiry,
                credential_expiry,
                decision_expiry,
                independence,
                responder,
                unlimited_scope,
                all_approver_roles,
                all_risks,
                operations,
            )| Case {
                request_expiry,
                credential_expiry,
                decision_expiry,
                independence,
                responder,
                unlimited_scope,
                all_approver_roles,
                all_risks,
                operations,
            },
        )
        .boxed();
    if max_steps < 2 {
        return general;
    }
    let warm = proptest::collection::vec(op, 0..=max_steps.min(128) - 2).prop_map(|suffix| {
        let mut case = base();
        case.operations = vec![
            Operation { kind: Kind::Approve, defect: Defect::None, time: 0 },
            Operation { kind: Kind::Consume, defect: Defect::ActionDigest, time: 0 },
        ];
        case.operations.extend(suffix);
        case
    });
    prop_oneof![2=>general,1=>warm].boxed()
}
fn op(kind: Kind, defect: Defect, time: u8) -> Operation {
    Operation { kind, defect, time }
}
pub fn calibration() -> Case {
    Case {
        operations: vec![
            op(Kind::Approve, Defect::None, 0),
            op(Kind::Consume, Defect::ActionDigest, 0),
        ],
        ..base()
    }
}
pub fn examples() -> Vec<Case> {
    let mut cases = vec![
        calibration(),
        Case {
            operations: vec![
                op(Kind::Approve, Defect::None, 0),
                op(Kind::Approve, Defect::None, 1),
                op(Kind::Deny, Defect::None, 1),
                op(Kind::Consume, Defect::None, 1),
                op(Kind::Consume, Defect::None, 1),
                op(Kind::Approve, Defect::None, 1),
            ],
            ..base()
        },
        Case {
            operations: vec![op(Kind::Expire, Defect::None, 4), op(Kind::Approve, Defect::None, 0)],
            ..base()
        },
        Case { operations: vec![op(Kind::Amend, Defect::None, 0)], ..base() },
        Case { unlimited_scope: true, all_approver_roles: true, all_risks: true, ..base() },
    ];
    for defect in [
        Defect::Signature,
        Defect::WrongSigner,
        Defect::RequestId,
        Defect::RequestDigest,
        Defect::RegistryRevision,
        Defect::Generation,
        Defect::Disabled,
        Defect::Environment,
        Defect::Workspace,
        Defect::Role,
        Defect::Tier,
        Defect::Actor,
    ] {
        cases.push(Case {
            operations: vec![op(Kind::Approve, defect, 0), op(Kind::Approve, Defect::None, 0)],
            ..base()
        });
    }
    for responder in 1..=4 {
        cases.push(Case {
            responder,
            operations: vec![op(Kind::Approve, Defect::None, 0)],
            ..base()
        });
    }
    cases
}
fn err(e: impl std::fmt::Debug) -> String {
    format!("{e:?}")
}
fn bytes(n: u8) -> [u8; 16] {
    [n; 16]
}
fn actor(n: u8) -> Result<ActorId, String> {
    ActorId::new(bytes(n)).map_err(err)
}
fn instant(tick: u64) -> AuthorityInstant {
    AuthorityInstant::new(Generation::first(), tick)
}
fn window(start: u64, end: u64) -> Result<ValidityWindow, String> {
    ValidityWindow::new(instant(start), instant(end)).map_err(err)
}
fn digest(n: u8) -> Sha256Digest {
    Sha256Digest::new([n; 32])
}
fn revision() -> Result<RevisionTuple, String> {
    Ok(RevisionTuple::new(
        AcceptanceSpecId::new(bytes(31)).map_err(err)?,
        HarnessId::new(bytes(32)).map_err(err)?,
        WorkspaceId::new(bytes(33)).map_err(err)?,
        Generation::first(),
        RevisionNumber::first(),
        PolicyId::new(bytes(34)).map_err(err)?,
        ProviderProfileId::new(bytes(35)).map_err(err)?,
    ))
}
fn selector(revision: RevisionTuple) -> ScopeSelector {
    ScopeSelector::new(
        ActorSelector::any_within_parent(),
        RoleSelector::any_within_parent(),
        EnvironmentSelector::any_within_parent(),
        PermissionSelector::any_within_parent(),
        revision,
    )
}
fn permissions() -> Result<PermissionSet, String> {
    PermissionSet::new(vec![Permission::new(
        ResourceId::new(bytes(36)).map_err(err)?,
        CapabilityName::new("workspace.patch".into()).map_err(err)?,
    )])
    .map_err(err)
}

/// Reusable constructor for the journal lane; this returns input data, never expected behavior.
pub fn fixture_request(case: &Case) -> Result<ApprovalRequest, String> {
    let revision = revision()?;
    let environment = EnvironmentId::new(bytes(37)).map_err(err)?;
    let boundary_uses = if case.unlimited_scope {
        UseLimit::unlimited()
    } else {
        UseLimit::limited(2).map_err(err)?
    };
    let scope_uses = if case.unlimited_scope {
        UseLimit::unlimited()
    } else {
        UseLimit::limited(1).map_err(err)?
    };
    let bound = AuthorityBoundary::new(
        vec![actor(2)?],
        vec![ActorRole::Writer],
        vec![environment],
        permissions()?,
        revision,
        window(0, 200)?,
        boundary_uses,
    )
    .map_err(err)?;
    let ceiling = AuthorityCeiling::new(
        bound,
        vec![CeilingGrant::new(digest(41), selector(revision), window(0, 200)?, scope_uses)],
        Vec::new(),
    )
    .map_err(err)?;
    let independent = [
        IndependenceRequirement::NotRequester,
        IndependenceRequirement::NotActionActor,
        IndependenceRequirement::NoProducingAttemptParticipation,
        IndependenceRequirement::NoReviewParticipation,
    ]
    .into_iter()
    .enumerate()
    .filter_map(|(i, requirement)| ((case.independence & (1 << i)) != 0).then_some(requirement))
    .collect();
    let approver_roles = if case.all_approver_roles {
        vec![
            ActorRole::Writer,
            ActorRole::Fixer,
            ActorRole::Reviewer,
            ActorRole::Evaluator,
            ActorRole::GateRunner,
            ActorRole::Orchestrator,
            ActorRole::EvolutionAgent,
            ActorRole::HumanAuthority,
            ActorRole::DaemonService,
            ActorRole::ProviderToolWorker,
            ActorRole::Plugin,
        ]
    } else {
        vec![ActorRole::HumanAuthority]
    };
    let requirement = ApprovalRequirement::new(
        AuthorityTier::User,
        approver_roles,
        IndependenceSet::new(independent).map_err(err)?,
        window(10, 150)?,
    )
    .map_err(err)?;
    let rules = RestrictionLayer::new(
        PolicyTier::Project,
        vec![RestrictionRule::require_approval(digest(42), selector(revision), requirement)],
    )
    .map_err(err)?;
    let risks = if case.all_risks {
        vec![
            RiskClass::Read,
            RiskClass::ScopedWrite,
            RiskClass::Execution,
            RiskClass::Network,
            RiskClass::DependencyEnvironment,
            RiskClass::RepositoryHistoryMutation,
            RiskClass::SecretUse,
            RiskClass::ExternalSideEffect,
            RiskClass::PolicyAuthority,
            RiskClass::HarnessPromotion,
        ]
    } else {
        vec![RiskClass::ScopedWrite]
    };
    let descriptor = OperationDescriptor::new(
        CapabilityName::new("workspace.patch".into()).map_err(err)?,
        OperationClass::WorkspaceMutation,
        RiskSet::new(risks).map_err(err)?,
    )
    .map_err(err)?;
    let policy = PolicyDefinition::new(
        revision.policy_id(),
        ceiling,
        OperationRegistry::new(vec![descriptor]).map_err(err)?,
        vec![rules],
    )
    .map_err(err)?;
    let scope = CapabilityScope::new(
        actor(2)?,
        ActorRole::Writer,
        environment,
        permissions()?,
        revision,
        window(0, 200)?,
        scope_uses,
    );
    let challenge = policy
        .evaluate(
            AuthorizationRequest::new(scope),
            AuthorityTimeState::new(instant(10)),
            instant(10),
        )
        .map_err(err)?
        .into_parts()
        .1
        .ok_or("policy did not yield expected approval challenge")?;
    ApprovalRequest::new(
        ApprovalRequestId::new(bytes(38)).map_err(err)?,
        ActionId::new(bytes(39)).map_err(err)?,
        ActionDigest::from_sha256(digest(43)),
        actor(1)?,
        ActorRole::Writer,
        challenge,
        digest(44),
        ParticipantSet::producing(vec![actor(3)?]).map_err(err)?,
        ParticipantSet::review(vec![actor(4)?]).map_err(err)?,
        window(10, u64::from(case.request_expiry))?,
    )
    .map_err(err)
}
pub fn fixture_registry(case: &Case, defect: Defect) -> Result<CredentialRegistrySnapshot, String> {
    let signing = SigningKey::from_bytes(&[77; 32]);
    let public = ApprovalPublicKey::new(signing.verifying_key().to_bytes());
    let credential = ApproverCredential::new(
        ApprovalKeyId::compute(public).map_err(err)?,
        public,
        actor(case.responder)?,
        ActorRole::HumanAuthority,
        EnvironmentId::new(bytes(if defect == Defect::Environment { 99 } else { 37 }))
            .map_err(err)?,
        WorkspaceId::new(bytes(if defect == Defect::Workspace { 99 } else { 33 })).map_err(err)?,
        if defect == Defect::Tier { AuthorityTier::Project } else { AuthorityTier::Organization },
        vec![ActorRole::HumanAuthority],
        window(10, u64::from(case.credential_expiry))?,
        Generation::first(),
        if defect == Defect::Disabled {
            CredentialStatus::Disabled
        } else {
            CredentialStatus::Enabled
        },
    )
    .map_err(err)?;
    CredentialRegistrySnapshot::new(RevisionNumber::first(), vec![credential]).map_err(err)
}
pub fn fixture_signed(
    request: &ApprovalRequest,
    case: &Case,
    choice: ApprovalChoice,
    defect: Defect,
) -> Result<SignedApprovalDecision, String> {
    let key = SigningKey::from_bytes(&[77; 32]);
    let public = ApprovalPublicKey::new(key.verifying_key().to_bytes());
    let wrong_request_digest = if defect == Defect::RequestDigest {
        let mut foreign = case.clone();
        foreign.request_expiry = if case.request_expiry == 111 { 112 } else { 111 };
        Some(fixture_request(&foreign)?.digest())
    } else {
        None
    };
    let decision = ApprovalDecision::new(
        CommandId::new(bytes(if choice == ApprovalChoice::Deny { 46 } else { 45 })).map_err(err)?,
        actor(if defect == Defect::Actor { 99 } else { case.responder })?,
        if defect == Defect::Role { ActorRole::Reviewer } else { ActorRole::HumanAuthority },
        if defect == Defect::RequestId {
            ApprovalRequestId::new(bytes(99)).map_err(err)?
        } else {
            request.request_id()
        },
        wrong_request_digest.unwrap_or_else(|| request.digest()),
        choice,
        instant(u64::from(case.decision_expiry)),
        ApprovalKeyId::compute(public).map_err(err)?,
        Generation::new(if defect == Defect::Generation { 2 } else { 1 }).map_err(err)?,
        RevisionNumber::new(if defect == Defect::RegistryRevision { 2 } else { 1 }).map_err(err)?,
    )
    .map_err(err)?;
    let signing =
        if defect == Defect::WrongSigner { SigningKey::from_bytes(&[78; 32]) } else { key };
    let mut signature = signing.sign(&decision.signing_bytes().map_err(err)?).to_bytes();
    if defect == Defect::Signature {
        signature[31] ^= 1;
    }
    Ok(SignedApprovalDecision::new(decision, ApprovalSignature::new(signature)))
}

fn amendment(request: &ApprovalRequest) -> Result<AmendmentIdentity, String> {
    AmendmentIdentity::new(
        request.scope().revision().policy_id(),
        PolicyId::new(bytes(98)).map_err(err)?,
        PolicyTier::Project,
        digest(97),
    )
    .map_err(err)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Pending,
    Approved,
    AmendmentAuthorized,
    Denied,
    Consumed,
    Cancelled,
    Expired,
}
struct Model {
    phase: Phase,
    floor: u64,
    resolution: Option<ApprovalChoice>,
}
fn observed(case: &Case, selector: u8, floor: u64) -> (u64, u64) {
    let expiry = u64::from(
        case.request_expiry.min(case.credential_expiry).min(case.decision_expiry).min(150),
    );
    match selector % 10 {
        0 => (1, 10),
        1 => (1, floor.saturating_add(1)),
        2 => (1, floor.saturating_sub(1)),
        3 => (1, expiry.saturating_sub(1)),
        4 => (1, expiry),
        5 => (1, expiry + 1),
        6 => (2, 10),
        7 => (1, 0),
        8 => (1, u64::from(case.request_expiry)),
        _ => (1, 200),
    }
}
fn verify_expected(case: &Case, defect: Defect, epoch: u64, tick: u64, floor: u64) -> bool {
    let conflict = case.responder <= 4 && (case.independence & (1 << (case.responder - 1))) != 0;
    let semantic_defect = matches!(defect, Defect::None | Defect::Action | Defect::ActionDigest);
    semantic_defect
        && !conflict
        && epoch == 1
        && tick >= floor
        && tick >= 10
        && tick < u64::from(case.request_expiry)
        && tick < u64::from(case.credential_expiry)
        && tick < u64::from(case.decision_expiry)
        && tick < 150
}

pub fn check(case: &Case) -> Result<crate::Metrics, String> {
    if case.operations.len() > 128
        || case.request_expiry <= 10
        || case.credential_expiry <= 10
        || case.decision_expiry <= 10
        || !(1..=5).contains(&case.responder)
    {
        return Err("invalid approval fixture bounds".into());
    }
    let request = fixture_request(case)?;
    let encoded = encode_approval_request(&request).map_err(err)?;
    let decoded = decode_approval_request(&encoded).map_err(err)?;
    if decoded != request {
        return Err("approval request codec lost exact authority bindings".into());
    }
    let mut trailing = encoded.clone();
    trailing.push(0);
    if decode_approval_request(&trailing).is_ok()
        || decode_approval_request(&encoded[..encoded.len() - 1]).is_ok()
    {
        return Err("approval request codec accepted trailing/truncated bytes".into());
    }
    let mut aggregate = ApprovalAggregate::new(request);
    super::approval_render::check(&aggregate)?;
    let mut model = Model { phase: Phase::Pending, floor: 10, resolution: None };
    let mut accepted = 0;
    let mut rejected = 0;
    let mut verified = 0;
    let mut invalid_signatures = 0;
    let mut consumed = 0;
    let mut replays = 0;
    let mut render_checks = 1;
    for (index, op) in case.operations.iter().enumerate() {
        let before = format!("{aggregate:?}");
        let (epoch, tick) = observed(case, op.time, model.floor);
        let now = AuthorityInstant::new(Generation::new(epoch).map_err(err)?, tick);
        let mut expect_accept = false;
        let mut next_phase = model.phase;
        let mut next_floor = model.floor;
        match op.kind {
            Kind::Approve | Kind::Amend | Kind::Deny => {
                let choice = match op.kind {
                    Kind::Approve => ApprovalChoice::ApproveOnce,
                    Kind::Amend => ApprovalChoice::Amend(amendment(aggregate.request())?),
                    Kind::Deny => ApprovalChoice::Deny,
                    Kind::Consume | Kind::Cancel | Kind::Expire => unreachable!(),
                };
                let registry = fixture_registry(case, op.defect)?;
                let signed = fixture_signed(aggregate.request(), case, choice, op.defect)?;
                let raw = encode_signed_decision(&signed).map_err(err)?;
                if decode_signed_decision(&raw).map_err(err)? != signed {
                    return Err("signed approval codec drift".into());
                }
                let authentication =
                    verify_signed_decision(aggregate.request(), &signed, &registry, now);
                let expected_auth = verify_expected(case, op.defect, epoch, tick, model.floor);
                if authentication.is_ok() != expected_auth {
                    return Err(format!(
                        "approval authentication divergence at {index}: {op:?}, expected={expected_auth}, result={authentication:?}"
                    ));
                }
                if let Ok(observation) = authentication {
                    verified += 1;
                    expect_accept =
                        model.phase == Phase::Pending || model.resolution == Some(choice);
                    if model.phase == Phase::Pending {
                        next_phase = match choice {
                            ApprovalChoice::Deny => Phase::Denied,
                            ApprovalChoice::ApproveOnce => Phase::Approved,
                            ApprovalChoice::Amend(_) => Phase::AmendmentAuthorized,
                        };
                        next_floor = tick;
                    }
                    let outcome = aggregate.resolve(observation, &registry);
                    aggregate = match outcome {
                        Ok(value) => {
                            if !expect_accept {
                                return Err("conflicting approval resolution accepted".into());
                            }
                            if model.phase != Phase::Pending {
                                replays += 1;
                            } else {
                                model.resolution = Some(choice);
                            }
                            value.into_parts().0
                        }
                        Err(failure) => {
                            if expect_accept {
                                return Err(format!(
                                    "valid approval resolution rejected: {:?}",
                                    failure.error()
                                ));
                            }
                            failure.into_parts().1
                        }
                    };
                } else {
                    invalid_signatures +=
                        u64::from(matches!(op.defect, Defect::Signature | Defect::WrongSigner));
                }
            }
            Kind::Consume => {
                let valid_until = u64::from(
                    case.request_expiry
                        .min(case.credential_expiry)
                        .min(case.decision_expiry)
                        .min(150),
                );
                expect_accept = model.phase == Phase::Approved
                    && !matches!(op.defect, Defect::Action | Defect::ActionDigest)
                    && epoch == 1
                    && tick >= model.floor
                    && tick < valid_until;
                let action = if op.defect == Defect::Action {
                    ActionId::new(bytes(99)).map_err(err)?
                } else {
                    aggregate.request().action_id()
                };
                let action_digest = if op.defect == Defect::ActionDigest {
                    ActionDigest::from_sha256(digest(99))
                } else {
                    aggregate.request().action_digest()
                };
                aggregate = match aggregate.consume_once(action, action_digest, now) {
                    Ok(value) => {
                        if !expect_accept {
                            return Err(
                                "approval consumed with stale/mismatched/duplicate authority"
                                    .into(),
                            );
                        }
                        if value.transition().action_id()
                            != ActionId::new(bytes(39)).map_err(err)?
                            || value.transition().action_digest()
                                != ActionDigest::from_sha256(digest(43))
                        {
                            return Err("consumed action lost exact bindings".into());
                        }
                        consumed += 1;
                        next_phase = Phase::Consumed;
                        next_floor = tick;
                        value.into_parts().0
                    }
                    Err(failure) => {
                        if expect_accept {
                            return Err(format!(
                                "valid approval use rejected: {:?}",
                                failure.error()
                            ));
                        }
                        failure.into_parts().1
                    }
                };
            }
            Kind::Cancel => {
                expect_accept = model.phase == Phase::Pending;
                aggregate = match aggregate.cancel() {
                    Ok(value) => {
                        if !expect_accept {
                            return Err("resolved approval cancelled".into());
                        }
                        next_phase = Phase::Cancelled;
                        value.into_parts().0
                    }
                    Err(failure) => {
                        if expect_accept {
                            return Err("pending cancellation rejected".into());
                        }
                        failure.into_parts().1
                    }
                };
            }
            Kind::Expire => {
                let expiry = if matches!(model.phase, Phase::Approved | Phase::AmendmentAuthorized)
                {
                    case.request_expiry
                        .min(case.credential_expiry)
                        .min(case.decision_expiry)
                        .min(150)
                } else {
                    case.request_expiry.min(150)
                };
                expect_accept = matches!(
                    model.phase,
                    Phase::Pending | Phase::Approved | Phase::AmendmentAuthorized
                ) && epoch == 1
                    && tick >= model.floor
                    && tick >= u64::from(expiry);
                aggregate = match aggregate.expire(now) {
                    Ok(value) => {
                        if !expect_accept {
                            return Err("approval expired outside permitted interval/phase".into());
                        }
                        next_phase = Phase::Expired;
                        next_floor = tick;
                        value.into_parts().0
                    }
                    Err(failure) => {
                        if expect_accept {
                            return Err(format!(
                                "valid approval expiry rejected: {:?}",
                                failure.error()
                            ));
                        }
                        failure.into_parts().1
                    }
                };
            }
        }
        if expect_accept {
            accepted += 1;
            model.phase = next_phase;
            model.floor = next_floor;
        } else {
            rejected += 1;
            if format!("{aggregate:?}") != before {
                return Err("rejected approval operation changed linear state".into());
            }
        }
        let expected_phase = match model.phase {
            Phase::Pending => ApprovalPhase::Pending,
            Phase::Approved => ApprovalPhase::ApprovedOnce,
            Phase::AmendmentAuthorized => ApprovalPhase::AmendmentAuthorized,
            Phase::Denied => ApprovalPhase::Denied,
            Phase::Consumed => ApprovalPhase::Consumed,
            Phase::Cancelled => ApprovalPhase::Cancelled,
            Phase::Expired => ApprovalPhase::Expired,
        };
        if aggregate.phase() != expected_phase
            || aggregate.request().authority_time().greatest_tick_millis() != model.floor
        {
            return Err(format!("approval lifecycle differs after {index}: {op:?}"));
        }
        super::approval_render::check(&aggregate)?;
        render_checks += 1;
    }
    Ok(crate::Metrics::from([
        ("accepted".into(), accepted),
        ("rejected".into(), rejected),
        ("authenticated".into(), verified),
        ("invalid_signatures".into(), invalid_signatures),
        ("consumed".into(), consumed),
        ("idempotent_replays".into(), replays),
        ("codec_checks".into(), 4),
        ("render_checks".into(), render_checks),
    ]))
}

/// Real C0 ownership wave reused by the journal campaign; pure approval cases stay storage-free.
pub fn durable_approval_wave() -> Result<crate::Metrics, String> {
    use peritus_journal::{
        AggregateId, AggregateKey, AggregateKind, AppendRequest, ApprovalCommitRequest,
        CredentialRegistryInstall, EventDraft, ExactFrame, HeadExpectation, JournalErrorKind,
        SqliteJournal, SqliteJournalOptions, StoreId,
    };
    use peritus_types::{EventId, EventSequence};
    let temporary =
        tempfile::Builder::new().prefix("peritus-stage4-approval-c0-").tempdir().map_err(err)?;
    let path = temporary.path().join("state.sqlite");
    let store_id = StoreId::new(bytes(80)).map_err(err)?;
    let key = AggregateKey::new(AggregateKind::Approval, AggregateId::new(bytes(81)).map_err(err)?);
    let mut journal =
        SqliteJournal::open(&path, store_id, SqliteJournalOptions::default()).map_err(err)?;
    let case = base();
    let registry = fixture_registry(&case, Defect::None)?;
    journal
        .commit_credential_registry(
            CredentialRegistryInstall::new(None, 1, &registry).map_err(err)?,
        )
        .map_err(err)?;
    let current = journal.current_credential_registry().map_err(err)?;
    let resolve = || -> Result<peritus_approval::ApprovalTransitionOutcome, String> {
        let aggregate = ApprovalAggregate::new(fixture_request(&case)?);
        let signed =
            fixture_signed(aggregate.request(), &case, ApprovalChoice::ApproveOnce, Defect::None)?;
        let observation =
            verify_signed_decision(aggregate.request(), &signed, &registry, instant(10))
                .map_err(err)?;
        aggregate.resolve(observation, &registry).map_err(|failure| err(failure.error()))
    };
    let append = |sequence: u64, head: HeadExpectation| -> Result<AppendRequest, String> {
        let command = CommandId::new(bytes(if sequence == 1 { 82 } else { 84 })).map_err(err)?;
        let event = EventId::new(bytes(if sequence == 1 { 83 } else { 85 })).map_err(err)?;
        let frame = ExactFrame::new(
            peritus_codec::encode_frame(
                65_020,
                1,
                b"approval-owner-wave",
                peritus_codec::CodecLimits::PRODUCTION,
            )
            .map_err(err)?,
        )
        .map_err(err)?;
        let event = EventDraft::new(
            key,
            EventSequence::new(sequence).map_err(err)?,
            event,
            if sequence == 1 { None } else { Some(EventId::new(bytes(83)).map_err(err)?) },
            frame,
            digest(88),
            Vec::new(),
        )
        .map_err(err)?;
        Ok(AppendRequest::new(
            store_id,
            command,
            digest(if sequence == 1 { 82 } else { 84 }),
            vec![head],
            vec![event],
            Vec::new(),
            Vec::new(),
            None,
            None,
            Vec::new(),
        ))
    };
    let first = journal
        .commit_approval_transition(
            ApprovalCommitRequest::new(
                append(1, HeadExpectation::Absent(key))?,
                resolve()?,
                None,
                Some(&current),
            )
            .map_err(err)?,
        )
        .map_err(err)?;
    if first.state_revision() != 1
        || first.aggregate().phase() != ApprovalPhase::ApprovedOnce
        || first.registry_binding() != Some((1, 1, registry.digest().map_err(err)?))
    {
        return Err("C0 approval receipt lost exact phase/CAS/registry binding".into());
    }
    let position = first.batch().last_position();
    let state_digest = first.state_digest();
    let original = journal
        .state_record(104, &bytes(38))
        .map_err(err)?
        .ok_or("C0 approval projection missing")?;
    let durable_bytes = original.bytes().to_vec();
    if original.revision() != 1
        || original.digest() != state_digest
        || original.producing_position() != position
    {
        return Err("C0 approval state projection disagrees with opaque commit receipt".into());
    }
    let prefix = b"PERITUS-C0-STATE\0";
    let header = prefix.len() + 12;
    if durable_bytes.len() < header + 100
        || &durable_bytes[..prefix.len()] != prefix
        || durable_bytes[prefix.len()..prefix.len() + 4] != [0, 1, 0, 4]
    {
        return Err("C0 approval state header/schema/kind mismatch".into());
    }
    let length =
        u64::from_be_bytes(durable_bytes[prefix.len() + 4..header].try_into().map_err(err)?);
    let payload = &durable_bytes[header..];
    if length != payload.len() as u64
        || payload[..16] != bytes(38)
        || payload[16..32] != bytes(39)
        || payload[32..64] != [43; 32]
        || payload[96..100] != [1, 1, 2, 2]
    {
        return Err(
            "C0 approval payload lost exact request/action or resolved phase semantics".into()
        );
    }
    let tail = payload
        .get(payload.len().saturating_sub(49)..)
        .ok_or("C0 approval registry tail missing")?;
    if tail.len() != 49
        || tail[0] != 1
        || tail[1..9] != 1_u64.to_be_bytes()
        || tail[9..17] != 1_u64.to_be_bytes()
        || tail[17..] != *registry.digest().map_err(err)?.as_bytes()
    {
        return Err("C0 approval durable registry tail differs".into());
    }
    let retry = journal
        .commit_approval_transition(
            ApprovalCommitRequest::new(
                append(1, HeadExpectation::Absent(key))?,
                resolve()?,
                None,
                Some(&current),
            )
            .map_err(err)?,
        )
        .map_err(err)?;
    if retry.batch().last_position() != position
        || retry.state_revision() != 1
        || retry.state_digest() != state_digest
        || journal.records_for_aggregate(key).map_err(err)?.len() != 1
    {
        return Err("same C0 approval command applied twice".into());
    }
    let aggregate = retry.into_parts().1;
    let signed =
        fixture_signed(aggregate.request(), &case, ApprovalChoice::ApproveOnce, Defect::None)?;
    let observation = verify_signed_decision(aggregate.request(), &signed, &registry, instant(10))
        .map_err(err)?;
    let outcome =
        aggregate.resolve(observation, &registry).map_err(|failure| err(failure.error()))?;
    let head = journal.head(key).map_err(err)?.ok_or("approval aggregate head missing")?;
    let stale = ApprovalCommitRequest::new(
        append(2, HeadExpectation::Present(head))?,
        outcome,
        Some(1),
        Some(&current),
    )
    .map_err(err)?;
    let credential = &registry.entries()[0];
    let rotated = ApproverCredential::new(
        credential.key_id(),
        credential.public_key(),
        credential.actor(),
        credential.principal_role(),
        credential.environment(),
        credential.workspace(),
        credential.maximum_tier(),
        credential.allowed_approval_roles().to_vec(),
        credential.validity(),
        credential.generation(),
        credential.status(),
    )
    .map_err(err)?;
    let newer =
        CredentialRegistrySnapshot::new(RevisionNumber::new(2).map_err(err)?, vec![rotated])
            .map_err(err)?;
    journal
        .commit_credential_registry(
            CredentialRegistryInstall::new(Some(1), 2, &newer).map_err(err)?,
        )
        .map_err(err)?;
    let failure = journal
        .commit_approval_transition(stale)
        .err()
        .ok_or("approval accepted stale opaque registry after rotation")?;
    if failure.kind() != JournalErrorKind::StaleRegistry {
        return Err(format!("stale approval registry wrong rejection: {failure:?}"));
    }
    if journal
        .state_record(104, &bytes(38))
        .map_err(err)?
        .ok_or("approval projection vanished")?
        .bytes()
        != durable_bytes
        || journal.records_for_aggregate(key).map_err(err)?.len() != 1
    {
        return Err("stale registry rejection partially committed approval".into());
    }
    drop(journal);
    let journal =
        SqliteJournal::open(&path, store_id, SqliteJournalOptions::default()).map_err(err)?;
    journal.validate_credential_registry_history().map_err(err)?;
    let reopened = journal
        .state_record(104, &bytes(38))
        .map_err(err)?
        .ok_or("reopened approval projection missing")?;
    let historical = journal
        .state_record_revision(104, &bytes(38), 1)
        .map_err(err)?
        .ok_or("approval immutable state history missing")?;
    if reopened.bytes() != durable_bytes
        || historical.bytes() != durable_bytes
        || reopened.revision() != 1
        || journal.current_credential_registry().map_err(err)?.revision() != 2
        || journal.records_for_aggregate(key).map_err(err)?.len() != 1
    {
        return Err("C0 approval reopen/history changed exact state or current registry".into());
    }
    Ok(crate::Metrics::from([
        ("typed_approval_commits".into(), 1),
        ("typed_approval_replays".into(), 1),
        ("stale_registry_rejections".into(), 1),
        ("typed_approval_reopens".into(), 1),
        ("credential_installs".into(), 2),
    ]))
}
