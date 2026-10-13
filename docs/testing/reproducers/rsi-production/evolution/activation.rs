//! Real F0 host authority adapter for the disposable native fixture only.
//! Fresh explicit CLI approval input is required separately for promotion and rollback.
#[path = "../../../../../crates/app/peritus-product-runner/src/developer_tools/command_runtime/contract.rs"]
mod actual_command_contract;
#[path = "f0.rs"]
mod fixture;
use ed25519_dalek::{Signer, SigningKey};
use peritus_approval::*;
use peritus_budget::*;
use peritus_codec::{CodecLimits, encode_frame, encode_message, sha256};
use peritus_daemon::rsi::evolution::*;
use peritus_evolution::*;
use peritus_journal::*;
use peritus_kernel::{CommandEnvelope, KernelAggregate, KernelCommand, KernelEvent, ReducerInputs};
use peritus_policy::*;
use peritus_protocol::KernelEventDto;
use peritus_spec::AcceptanceContract;
use peritus_types::*;
use std::{
    error::Error,
    fs,
    io::Read,
    path::{Path, PathBuf},
};
type Result<T> = std::result::Result<T, Box<dyn Error>>;
fn checked<T, E: std::fmt::Debug>(result: std::result::Result<T, E>) -> Result<T> {
    result.map_err(|e| format!("{e:?}").into())
}

fn identity(label: &str, seed: Sha256Digest) -> [u8; 16] {
    let mut p = b"peritus.actual-f0-human-host-v1\0".to_vec();
    p.extend_from_slice(seed.as_bytes());
    p.extend_from_slice(label.as_bytes());
    let mut b = [0; 16];
    b.copy_from_slice(&sha256(&p).as_bytes()[..16]);
    b[0] |= 1;
    b
}
fn cid(label: &str, seed: Sha256Digest) -> Result<CommandId> {
    CommandId::new(identity(label, seed)).map_err(|_| "command identity rejected".into())
}
fn eid(label: &str, seed: Sha256Digest) -> Result<EventId> {
    EventId::new(identity(label, seed)).map_err(|_| "event identity rejected".into())
}
fn time(t: u64) -> AuthorityInstant {
    AuthorityInstant::new(Generation::first(), t)
}
fn selector(r: RevisionTuple) -> ScopeSelector {
    ScopeSelector::new(
        ActorSelector::any_within_parent(),
        RoleSelector::any_within_parent(),
        EnvironmentSelector::any_within_parent(),
        PermissionSelector::any_within_parent(),
        r,
    )
}
fn permissions(resource: ResourceId) -> Result<PermissionSet> {
    Ok(checked(PermissionSet::new(vec![Permission::new(
        resource,
        CapabilityName::new("harness.promote".to_owned())
            .map_err(|_| "capability name rejected")?,
    )]))?)
}
fn policy(
    actor: ActorId,
    env: EnvironmentId,
    resource: ResourceId,
    r: RevisionTuple,
    seed: Sha256Digest,
    approval: bool,
) -> Result<PolicyDefinition> {
    let validity = checked(ValidityWindow::new(time(10), time(1_000_000)))?;
    let uses = checked(UseLimit::limited(1))?;
    let boundary = checked(AuthorityBoundary::new(
        vec![actor],
        vec![ActorRole::Orchestrator],
        vec![env],
        permissions(resource)?,
        r,
        validity,
        uses,
    ))?;
    let ceiling = checked(AuthorityCeiling::new(
        boundary,
        vec![CeilingGrant::new(seed, selector(r), validity, uses)],
        vec![],
    ))?;
    let layers = if approval {
        vec![checked(RestrictionLayer::new(
            PolicyTier::User,
            vec![RestrictionRule::require_approval(
                seed,
                selector(r),
                checked(ApprovalRequirement::new(
                    AuthorityTier::User,
                    vec![ActorRole::HumanAuthority],
                    checked(IndependenceSet::new(vec![
                        IndependenceRequirement::NotRequester,
                        IndependenceRequirement::NotActionActor,
                    ]))?,
                    validity,
                ))?,
            )],
        ))?]
    } else {
        vec![]
    };
    Ok(checked(PolicyDefinition::new(
        r.policy_id(),
        ceiling,
        checked(OperationRegistry::new(vec![checked(OperationDescriptor::new(
            CapabilityName::new("harness.promote".to_owned()).map_err(|_| "name rejected")?,
            OperationClass::HarnessPromotion,
            checked(RiskSet::new(vec![RiskClass::HarnessPromotion]))?,
        ))?]))?,
        layers,
    ))?)
}
fn scope(
    actor: ActorId,
    env: EnvironmentId,
    resource: ResourceId,
    r: RevisionTuple,
) -> Result<CapabilityScope> {
    Ok(CapabilityScope::new(
        actor,
        ActorRole::Orchestrator,
        env,
        permissions(resource)?,
        r,
        checked(ValidityWindow::new(time(10), time(1_000_000)))?,
        checked(UseLimit::limited(1))?,
    ))
}
fn append(
    j: &SqliteJournal,
    key: AggregateKey,
    seed: Sha256Digest,
    label: &str,
    frame: Vec<u8>,
    r: RevisionTuple,
) -> Result<AppendRequest> {
    let head = j.head(key)?;
    let seq = head.map_or(1, |h| h.sequence().get() + 1);
    let command = cid(label, seed)?;
    let event = eid(&format!("event-{label}"), seed)?;
    let digest = sha256(&frame);
    let draft = EventDraft::new(
        key,
        EventSequence::new(seq).map_err(|_| "event sequence rejected")?,
        event,
        head.map(AggregateHead::event_id),
        ExactFrame::new(frame)?,
        peritus_evidence::revision_digest(&r),
        vec![],
    )?;
    Ok(AppendRequest::new(
        j.store_id(),
        command,
        digest,
        vec![head.map_or(HeadExpectation::Absent(key), HeadExpectation::Present)],
        vec![draft],
        vec![],
        vec![],
        None,
        None,
        vec![],
    ))
}
fn kernel_append(
    j: &SqliteJournal,
    session: SessionId,
    e: CommandEnvelope,
    event: KernelEvent,
) -> Result<AppendRequest> {
    let key = AggregateKey::new(AggregateKind::Kernel, AggregateId::new(*session.as_bytes())?);
    let head = j.head(key)?;
    let frame = encode_message(&KernelEventDto::from(event), CodecLimits::PRODUCTION)?;
    let digest = sha256(&frame);
    let draft = EventDraft::new(
        key,
        event.sequence(),
        event.id(),
        event.previous_event_id(),
        ExactFrame::new(frame)?,
        peritus_evidence::revision_digest(&event.revision()),
        vec![],
    )?;
    Ok(AppendRequest::new(
        j.store_id(),
        e.command_id(),
        digest,
        vec![head.map_or(HeadExpectation::Absent(key), HeadExpectation::Present)],
        vec![draft],
        vec![],
        vec![],
        None,
        None,
        vec![],
    ))
}
fn next(
    j: &mut SqliteJournal,
    s: KernelAggregate,
    contract: &AcceptanceContract,
    seed: Sha256Digest,
    label: &str,
    cmd: KernelCommand,
    inputs: ReducerInputs<'_>,
    refs: Vec<KernelInputReference>,
) -> Result<CommittedKernelTransition> {
    let e = CommandEnvelope::new(
        cid(label, seed)?,
        eid(&format!("event-{label}"), seed)?,
        Some(s.head_event_id()),
        s.revision(),
    );
    let session = s.session().id();
    let t = s.reduce(e, cmd, inputs).into_result().map_err(|x| format!("kernel {label}: {x:?}"))?;
    let request =
        KernelCommitRequest::transition(kernel_append(j, session, e, t.event())?, t, e, cmd, refs)?;
    let _ = contract;
    Ok(j.commit_kernel_transition(request)?)
}
fn dispatch(
    j: &mut SqliteJournal,
    project: ProjectId,
    contract: &AcceptanceContract,
    r: RevisionTuple,
    seed: Sha256Digest,
    actor: ActorId,
    env: EnvironmentId,
    action: ActionId,
    action_digest: Sha256Digest,
    cap: &CapabilityUseTransition,
) -> Result<CommittedKernelTransition> {
    let session = SessionId::new(identity("session", seed)).map_err(|_| "session rejected")?;
    let run = RunId::new(identity("run", seed)).map_err(|_| "run rejected")?;
    let attempt = AttemptId::new(identity("attempt", seed)).map_err(|_| "attempt rejected")?;
    let turn = TurnId::new(identity("turn", seed)).map_err(|_| "turn rejected")?;
    let e = CommandEnvelope::new(cid("open", seed)?, eid("event-open", seed)?, None, r);
    let genesis = KernelAggregate::open(project, session, contract, r, e)
        .map_err(|x| format!("kernel open:{x:?}"))?;
    let mut state = j
        .commit_kernel_transition(KernelCommitRequest::genesis(
            kernel_append(j, session, e, genesis.event())?,
            genesis,
            e,
            vec![],
        )?)?
        .into_parts()
        .1;
    let root = BudgetId::new(identity("root-budget", seed)).map_err(|_| "budget rejected")?;
    let child = BudgetId::new(identity("child-budget", seed)).map_err(|_| "budget rejected")?;
    let ledger = BudgetLedger::new_root(
        root,
        r,
        BudgetLimits::new(BudgetAmounts::from_units(100, 1000, 10000, 10, 5)),
    )
    .transition(BudgetCommand::AllocateChild(ChildBudgetRequest::new(
        child,
        root,
        r,
        BudgetLimits::new(BudgetAmounts::from_units(40, 400, 5000, 4, 2)),
    )))
    .map_err(|x| format!("actual budget:{x:?}"))?
    .into_ledger();
    let root_budget = ledger.account(root).map_err(|x| format!("root budget:{x:?}"))?;
    let child_budget = ledger.account(child).map_err(|x| format!("child budget:{x:?}"))?;
    state = next(
        j,
        state,
        contract,
        seed,
        "start-run",
        KernelCommand::StartRun { run_id: run },
        ReducerInputs::new(contract).with_run_budget(root_budget),
        vec![],
    )?
    .into_parts()
    .1;
    state = next(
        j,
        state,
        contract,
        seed,
        "start-attempt",
        KernelCommand::StartAttempt { run_id: run, attempt_id: attempt },
        ReducerInputs::new(contract)
            .with_attempt_budget(child_budget)
            .with_parent_budget(root_budget),
        vec![],
    )?
    .into_parts()
    .1;
    state = next(
        j,
        state,
        contract,
        seed,
        "start-turn",
        KernelCommand::StartTurn { attempt_id: attempt, turn_id: turn },
        ReducerInputs::new(contract),
        vec![],
    )?
    .into_parts()
    .1;
    state = next(
        j,
        state,
        contract,
        seed,
        "propose",
        KernelCommand::ProposeAction {
            turn_id: turn,
            action_id: action,
            digest: action_digest,
            actor_id: actor,
            role: ActorRole::Orchestrator,
            environment_id: env,
        },
        ReducerInputs::new(contract),
        vec![],
    )?
    .into_parts()
    .1;
    let reference = KernelInputReference::new(
        1,
        cap.successor().issuance_command_id().as_bytes().to_vec(),
        cap.transition_digest(),
    )?;
    state = next(
        j,
        state,
        contract,
        seed,
        "authorize",
        KernelCommand::AuthorizeAction { action_id: action },
        ReducerInputs::new(contract).with_capability_use(cap),
        vec![reference],
    )?
    .into_parts()
    .1;
    next(
        j,
        state,
        contract,
        seed,
        "dispatch",
        KernelCommand::DispatchAction { action_id: action },
        ReducerInputs::new(contract),
        vec![],
    )
}
struct ActualAuthority {
    action: ActionId,
    kernel: CommittedKernelTransition,
    capability: CommittedCapabilityUse,
    approval: ApprovalUseOutcome,
    revision: u64,
    registry: CurrentCredentialRegistry,
}
fn authority(
    j: &mut SqliteJournal,
    root: &Path,
    project: ProjectId,
    r: RevisionTuple,
    proposal: Sha256Digest,
    label: &str,
    contract: &AcceptanceContract,
    explicit_input: &str,
) -> Result<ActualAuthority> {
    let host_seed = sha256(format!("{}:{label}", fixture::hex(proposal.as_bytes())).as_bytes());
    let actor =
        ActorId::new(identity("host-orchestrator", host_seed)).map_err(|_| "actor rejected")?;
    let human = ActorId::new(identity("ephemeral-approved-human-fixture", host_seed))
        .map_err(|_| "human rejected")?;
    let env = EnvironmentId::new(identity("approval-environment", host_seed))
        .map_err(|_| "environment rejected")?;
    let resource = ResourceId::new(*project.as_bytes()).map_err(|_| "project resource rejected")?;
    let action = ActionId::new(identity(label, host_seed)).map_err(|_| "action rejected")?;
    let grant = policy(actor, env, resource, r, proposal, false)?
        .evaluate(
            AuthorizationRequest::new(scope(actor, env, resource, r)?),
            AuthorityTimeState::new(time(10)),
            time(10),
        )
        .map_err(|x| format!("host capability:{x:?}"))?
        .into_parts()
        .0
        .ok_or("host-admitted exact capability denied")?;
    let cap = grant
        .issue(cid("issue", host_seed)?, proposal)
        .into_capability()
        .try_use(
            CapabilityUseRequest::new(
                action,
                proposal,
                Permission::new(
                    resource,
                    CapabilityName::new("harness.promote".to_owned())
                        .map_err(|_| "capability rejected")?,
                ),
                actor,
                ActorRole::Orchestrator,
                env,
                r,
                time(20),
            ),
            proposal,
        )
        .map_err(|x| format!("actual capability use:{x:?}"))?;
    let kernel = dispatch(j, project, contract, r, host_seed, actor, env, action, proposal, &cap)?;
    let key = AggregateKey::new(
        AggregateKind::Approval,
        AggregateId::new(identity("capability-owner", host_seed))?,
    );
    let cap_bytes = serde_json::to_vec(
        &serde_json::json!({"schema":"actual-host-capability-use-v1","action":fixture::hex(action.as_bytes()),"proposal":fixture::hex(proposal.as_bytes()),"transition":fixture::hex(cap.transition_digest().as_bytes()),"scope":"harness.promote","actual_explicit_input":explicit_input}),
    )?;
    let capability = j.commit_capability_use(CapabilityCommitRequest::new(
        append(
            j,
            key,
            host_seed,
            "capability-commit",
            encode_frame(
                peritus_protocol::schema::AUTHORITY_OWNER_RECORD_FAMILY,
                1,
                &cap_bytes,
                CodecLimits::PRODUCTION,
            )?,
            r,
        )?,
        cap,
        None,
    )?)?;
    let challenge = policy(actor, env, resource, r, proposal, true)?
        .evaluate(
            AuthorizationRequest::new(scope(actor, env, resource, r)?),
            AuthorityTimeState::new(time(20)),
            time(20),
        )
        .map_err(|x| format!("approval challenge:{x:?}"))?
        .into_parts()
        .1
        .ok_or("exact approval challenge absent")?;
    let request = checked(ApprovalRequest::new(
        ApprovalRequestId::new(identity("request", host_seed)).map_err(|_| "request rejected")?,
        action,
        ActionDigest::from_sha256(proposal),
        actor,
        ActorRole::Orchestrator,
        challenge,
        sha256(explicit_input.as_bytes()),
        checked(ParticipantSet::producing(vec![actor]))?,
        checked(ParticipantSet::review(vec![]))?,
        checked(ValidityWindow::new(time(20), time(1_000_000)))?,
    ))?;
    let mut secret = [0; 32];
    fs::File::open("/dev/urandom")?.read_exact(&mut secret)?;
    let signing = SigningKey::from_bytes(&secret);
    secret.fill(0);
    let public = ApprovalPublicKey::new(signing.verifying_key().to_bytes());
    let key_id = checked(ApprovalKeyId::compute(public))?;
    let current = j.current_credential_registry().ok();
    let expected = current.as_ref().map(CurrentCredentialRegistry::revision);
    let registry_revision = expected.unwrap_or(0) + 1;
    let generation = current.as_ref().map_or(1, |x| x.generation() + 1);
    let snapshot = checked(CredentialRegistrySnapshot::new(
        RevisionNumber::new(registry_revision).map_err(|_| "registry revision rejected")?,
        vec![checked(ApproverCredential::new(
            key_id,
            public,
            human,
            ActorRole::HumanAuthority,
            env,
            r.workspace_id(),
            AuthorityTier::User,
            vec![ActorRole::HumanAuthority],
            checked(ValidityWindow::new(time(10), time(1_000_000)))?,
            Generation::new(generation).map_err(|_| "generation rejected")?,
            CredentialStatus::Enabled,
        ))?],
    ))?;
    j.commit_credential_registry(CredentialRegistryInstall::new(expected, generation, &snapshot)?)?;
    let registry = j.current_credential_registry()?;
    // Only public credential material is persisted; native restart must use this exact owner snapshot.
    fs::write(root.join("native-approval-registry.bin"), registry.snapshot_payload()?)?;
    fs::write(
        root.join("native-approval-registry-generation.json"),
        serde_json::to_vec_pretty(
            &serde_json::json!({"generation":registry.generation(),"revision":registry.revision(),"digest":fixture::hex(registry.digest().as_bytes())}),
        )?,
    )?;
    let decision = checked(ApprovalDecision::new(
        cid("fresh-explicit-approval", host_seed)?,
        human,
        ActorRole::HumanAuthority,
        request.request_id(),
        request.digest(),
        ApprovalChoice::ApproveOnce,
        time(1_000_000),
        key_id,
        Generation::new(generation).map_err(|_| "generation rejected")?,
        snapshot.revision(),
    ))?;
    let signature = signing.sign(&checked(decision.signing_bytes())?).to_bytes();
    let signed = SignedApprovalDecision::new(decision, ApprovalSignature::new(signature));
    let request_bytes = checked(encode_approval_request(&request))?;
    let signed_bytes = checked(encode_signed_decision(&signed))?;
    let (bad_decision, _) = checked(decode_signed_decision(&signed_bytes))?.into_parts();
    let bad = SignedApprovalDecision::new(bad_decision, ApprovalSignature::new([0; 64]));
    if checked(verify_signed_decision(&request, &bad, &snapshot, time(30))).is_ok() {
        return Err("invalid signature authenticated".into());
    }
    let observation = checked(verify_signed_decision(&request, &signed, &snapshot, time(30)))?;
    let outcome = ApprovalAggregate::new(request)
        .resolve(observation, &snapshot)
        .map_err(|x| format!("actual approval resolution:{:?}", x.error()))?;
    let approval_key = AggregateKey::new(
        AggregateKind::Approval,
        AggregateId::new(identity("approval-owner", host_seed))?,
    );
    let approval = j.commit_approval_transition(ApprovalCommitRequest::new(
        append(
            j,
            approval_key,
            host_seed,
            "approved-once",
            encode_frame(
                peritus_protocol::schema::AUTHORITY_OWNER_RECORD_FAMILY,
                1,
                &signed_bytes,
                CodecLimits::PRODUCTION,
            )?,
            r,
        )?,
        outcome,
        None,
        Some(&registry),
    )?)?;
    let revision = approval.state_revision();
    fs::create_dir_all(root.join("actual-human-approvals"))?;
    fs::write(
        root.join("actual-human-approvals").join(format!("{label}-request.bin")),
        request_bytes,
    )?;
    fs::write(
        root.join("actual-human-approvals").join(format!("{label}-signed.bin")),
        signed_bytes,
    )?;
    let approved = approval.into_parts().1;
    // Wrong action consumption must retain the exact approved-once state and leave the pointer alone.
    let wrong =
        ActionId::new(identity("wrong-action", proposal)).map_err(|_| "wrong action rejected")?;
    let failure = approved
        .consume_once(wrong, ActionDigest::from_sha256(proposal), time(40))
        .err()
        .ok_or("wrong action consumed approval")?;
    let approved = failure.into_parts().1;
    let approval = approved
        .consume_once(action, ActionDigest::from_sha256(proposal), time(40))
        .map_err(|x| format!("actual approve-once use:{:?}", x.error()))?;
    Ok(ActualAuthority { action, kernel, capability, approval, revision, registry })
}
fn reject_reused(
    j: &mut SqliteJournal,
    root: &Path,
    label: &str,
    proposal: Sha256Digest,
    r: RevisionTuple,
    prior_revision: u64,
    registry: &CurrentCredentialRegistry,
) -> Result<()> {
    let request = checked(decode_approval_request(&fs::read(
        root.join("actual-human-approvals").join(format!("{label}-request.bin")),
    )?))?;
    let signed = checked(decode_signed_decision(&fs::read(
        root.join("actual-human-approvals").join(format!("{label}-signed.bin")),
    )?))?;
    let snapshot = checked(decode_credential_registry(registry.snapshot_payload()?))?;
    let action = request.action_id();
    let observation = checked(verify_signed_decision(&request, &signed, &snapshot, time(50)))?;
    let outcome = ApprovalAggregate::new(request)
        .resolve(observation, &snapshot)
        .map_err(|x| format!("replayed actual signature: {:?}", x.error()))?;
    let consumed = outcome
        .into_parts()
        .0
        .consume_once(action, ActionDigest::from_sha256(proposal), time(60))
        .map_err(|x| format!("replayed exact approved signature: {:?}", x.error()))?;
    let key = AggregateKey::new(
        AggregateKind::Approval,
        AggregateId::new(identity("reused-approval-attempt", proposal))?,
    );
    let frame = encode_frame(
        300,
        1,
        b"actual replay of an already consumed signed approval",
        CodecLimits::PRODUCTION,
    )?;
    let before = j.integrity_export()?.report().last_position();
    let request = ApprovalUseCommitRequest::new(
        append(j, key, proposal, "reused-approval", frame, r)?,
        consumed,
        prior_revision,
        registry,
    )?;
    let error =
        j.commit_approval_use(request).err().ok_or("already consumed approval was reused")?;
    if j.integrity_export()?.report().last_position() != before {
        return Err("reused approval appended an effect".into());
    }
    fs::write(
        root.join(format!("actual-{label}-approval-reuse-rejected.txt")),
        format!("{error:?}"),
    )?;
    Ok(())
}

fn run() -> Result<()> {
    let args = std::env::args().collect::<Vec<_>>();
    if args.len() < 4 {
        return Err("usage: activation-driver prepare|approve-promotion-once|prepare-rollback|approve-rollback-once FIXTURE_ROOT ACTUAL_E3_ID [EXACT_PROPOSAL_DIGEST]".into());
    }
    let root = fs::canonicalize(PathBuf::from(&args[2]))?;

    let c = peritus_eval::EvaluationCampaignId::new(fixture::parse(&args[3])?)
        .map_err(|_| "E3 identity rejected")?;
    let campaign = EvolutionCampaignId::new(fixture::id(&format!(
        "actual-f0-campaign:{}",
        fixture::hex(c.as_bytes())
    )))
    .map_err(|_| "campaign rejected")?;
    let (mut j, a, mut evidence, facts) = fixture::store(&root)?;
    if facts["fixture_only"] != true {
        return Err("activation consumer requires an owned disposable E1 fixture marker".into());
    }
    let project =
        ProjectId::new(fixture::key(&facts, "project_id")?).map_err(|_| "project rejected")?;
    let review = RunId::new(fixture::id("independent-native-candidate-review"))
        .map_err(|_| "review rejected")?;
    let mut op = EvolutionOperator::new(&mut j, &a, &mut evidence);
    if args[1] == "prepare" {
        let old = op.pointer(project)?.current();
        let p = op.prepare_promotion(campaign, Some(review), fixture::ids("prepare-promotion")?)?;
        if op.pointer(project)?.current() != old {
            return Err("preparation activated production".into());
        }
        let out = serde_json::json!({"mode":"promotion","proposal":fixture::hex(p.digest().as_bytes()),"pointer_unchanged":true,"approval_required":true});
        fs::write(root.join("actual-promotion-prepared.json"), serde_json::to_vec_pretty(&out)?)?;
        println!("{}", out);
        return Ok(());
    }
    if args[1] == "prepare-rollback" {
        let pointer = op.pointer(project)?;
        let target = pointer.history().first().ok_or("retained baseline absent")?.id();
        let old = pointer.current();
        let p = op.prepare_rollback(project, target, fixture::ids("prepare-rollback")?)?;
        if op.pointer(project)?.current() != old {
            return Err("rollback preparation changed production".into());
        }
        let out = serde_json::json!({"mode":"rollback","proposal":fixture::hex(p.digest().as_bytes()),"pointer_unchanged":true,"fresh_approval_required":true});
        fs::write(root.join("actual-rollback-prepared.json"), serde_json::to_vec_pretty(&out)?)?;
        println!("{}", out);
        return Ok(());
    }
    let pointer = op.pointer(project)?;
    let (proposal, r, label) = match (args[1].as_str(), pointer.pending()) {
        ("approve-promotion-once", Some(PendingActivation::Promotion(p))) => {
            (p.digest(), p.candidate().revision(), "promotion")
        }
        ("approve-rollback-once", Some(PendingActivation::Rollback(p))) => {
            (p.digest(), p.target().revision(), "rollback")
        }
        _ => return Err("no exact pending proposal for this explicit approval operation".into()),
    };
    if args.len() != 5 || fixture::parse::<32>(&args[4])? != *proposal.as_bytes() {
        return Err("fresh explicit input must name the exact current proposal digest".into());
    }
    let parent =
        RunId::new(fixture::id("host-admitted-E1-run")).map_err(|_| "actual E1 parent rejected")?;
    let contract =
        actual_command_contract::command_contract(parent, if label == "promotion" { 5 } else { 3 })
            .map_err(|x| format!("actual B2 contract:{x}"))?;
    if contract.id() != r.acceptance_spec_id() {
        return Err("actual B2 contract differs from prepared target".into());
    }
    let before = pointer.current();
    let generation = pointer.generation();
    drop(op);
    let input = serde_json::to_string(
        &serde_json::json!({"argv":args,"fixture_only":true,"user_authorization":"explicit disposable promotion and rollback within authorized real candidate testing","proposal":fixture::hex(proposal.as_bytes()),"decision":"ApproveOnce"}),
    )?;
    if label == "promotion" {
        let wrong_revision = fixture::tuple(&facts["baseline"]["revision"])?;
        let wrong_contract = actual_command_contract::command_contract(parent, 3)
            .map_err(|x| format!("actual wrong-scope contract: {x}"))?;
        let wrong = authority(
            &mut j,
            &root,
            project,
            wrong_revision,
            proposal,
            "wrong-revision",
            &wrong_contract,
            &input,
        )?;
        let mut check = EvolutionOperator::new(&mut j, &a, &mut evidence);
        let wrong_input = EvolutionActivationInput::new(
            wrong.action,
            &wrong.kernel,
            &wrong.capability,
            wrong.approval,
            wrong.revision,
            &wrong.registry,
        );
        let error = check
            .activate_promotion(campaign, wrong_input, fixture::ids("wrong-revision-activation")?)
            .err()
            .ok_or("wrong revision activated production")?;
        if check.pointer(project)?.current() != before {
            return Err("wrong revision rejection changed pointer".into());
        }
        fs::write(root.join("actual-wrong-revision-rejection.txt"), format!("{error:?}"))?;
        drop(check);
        let wrong_project = ProjectId::new(identity("wrong-project", proposal))
            .map_err(|_| "wrong project rejected")?;
        let wrong = authority(
            &mut j,
            &root,
            wrong_project,
            r,
            proposal,
            "wrong-project",
            &contract,
            &input,
        )?;
        let mut check = EvolutionOperator::new(&mut j, &a, &mut evidence);
        let wrong_input = EvolutionActivationInput::new(
            wrong.action,
            &wrong.kernel,
            &wrong.capability,
            wrong.approval,
            wrong.revision,
            &wrong.registry,
        );
        let error = check
            .activate_promotion(campaign, wrong_input, fixture::ids("wrong-project-activation")?)
            .err()
            .ok_or("wrong project activated production")?;
        if check.pointer(project)?.current() != before {
            return Err("wrong project rejection changed pointer".into());
        }
        fs::write(root.join("actual-wrong-project-rejection.txt"), format!("{error:?}"))?;
    }
    let auth = authority(&mut j, &root, project, r, proposal, label, &contract, &input)?;
    let mut op = EvolutionOperator::new(&mut j, &a, &mut evidence);
    if op.pointer(project)?.current() != before {
        return Err("authority preparation changed production before atomic activation".into());
    }
    let value = EvolutionActivationInput::new(
        auth.action,
        &auth.kernel,
        &auth.capability,
        auth.approval,
        auth.revision,
        &auth.registry,
    );
    let next = if label == "promotion" {
        op.activate_promotion(campaign, value, fixture::ids("activate-promotion")?)?
    } else {
        op.activate_rollback(project, value, fixture::ids("activate-rollback")?)?
    };
    if next.current().revision() != r
        || next.generation() != generation + 1
        || next.current() == before
        || next.pending().is_some()
    {
        return Err("actual atomic activation failed pointer invariants".into());
    }
    drop(op);
    reject_reused(&mut j, &root, label, proposal, r, auth.revision, &auth.registry)?;
    let out = serde_json::json!({"status":"PASS","mode":label,"proposal":fixture::hex(proposal.as_bytes()),"generation":next.generation(),"history_entries":next.history().len(),"current_binding":fixture::hex(next.current().digest().as_bytes()),"actual_kernel_batch":fixture::hex(auth.kernel.batch().batch_hash().as_bytes()),"actual_capability_batch":fixture::hex(auth.capability.batch().batch_hash().as_bytes()),"fresh_approve_once_consumed":true,"wrong_action_consumption_rejected":true,"future_invocation_oracle_pending":true});
    fs::write(
        root.join(format!("actual-{label}-activation.json")),
        serde_json::to_vec_pretty(&out)?,
    )?;
    println!("{}", serde_json::to_string_pretty(&out)?);
    Ok(())
}
fn main() {
    if let Err(e) = run() {
        eprintln!("actual activation consumer failed: {e:?}");
        std::process::exit(1)
    }
}
