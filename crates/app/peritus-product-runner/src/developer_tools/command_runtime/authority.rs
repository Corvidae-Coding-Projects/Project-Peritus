//! Exact C4 and C2 committed authority for one product command.

use std::path::Path;

use peritus_budget::{BudgetAmounts, BudgetCommand, BudgetLedger, BudgetLimits, BudgetRequest};
use peritus_codec::CodecLimits;
use peritus_journal::{
    AggregateKey, AggregateKind, BudgetCommitRequest, CapabilityCommitRequest,
    CommittedBudgetTransition, CommittedCapabilityUse, CommittedKernelTransition,
    CommittedLeaseTransition, CurrentAuthorityEpoch, ExpectedAuthorityEpoch, HeadExpectation,
    SqliteJournal,
};
use peritus_policy::{
    ActorRole, ActorSelector, AuthorityBoundary, AuthorityCeiling, AuthorityInstant,
    AuthorityTimeState, AuthorizationRequest, CapabilityScope, CapabilityUseRequest,
    CapabilityUseTransition, CeilingGrant, EnvironmentSelector, OperationClass,
    OperationDescriptor, OperationRegistry, Permission, PermissionSelector, PermissionSet,
    PolicyDefinition, RiskClass, RiskSet, RoleSelector, ScopeSelector, UseLimit, ValidityWindow,
};
use peritus_process::{
    EXECUTION_INTENT_MEDIA_TYPE, ExecutionAuthorizationRequest, ExecutionIntentPayload,
    ExecutionPlan,
};
use peritus_protocol::ActionIntentDto;
use peritus_spec::AcceptanceContract;
use peritus_tool_protocol::PreparedToolCall;
use peritus_tool_router::{ToolAuthorizationRequest, tool_action_intent};
use peritus_types::{Generation, Sha256Digest};

use super::{identity::CommandIds, journal, kernel, lease};

pub(super) struct ToolAuthority {
    intent: ActionIntentDto,
    kernel: CommittedKernelTransition,
    capability: CommittedCapabilityUse,
    budget: CommittedBudgetTransition,
    epoch: CurrentAuthorityEpoch,
}

impl ToolAuthority {
    pub(super) const fn request<'a>(
        &'a self,
        ids: &CommandIds,
        prepared: &PreparedToolCall,
    ) -> ToolAuthorizationRequest<'a> {
        ToolAuthorizationRequest::new(
            &self.intent,
            &self.kernel,
            &self.capability,
            &self.budget,
            None,
            &self.epoch,
            ids.revision,
            ids.session,
            instant(20),
            ids.revision.workspace_generation(),
            ids.revision.workspace_revision(),
            prepared.prepared_digest(),
        )
    }
}

pub(super) struct ProcessAuthority {
    intent: ActionIntentDto,
    kernel: CommittedKernelTransition,
    capability: CommittedCapabilityUse,
    budget: CommittedBudgetTransition,
    lease: CommittedLeaseTransition,
    epoch: CurrentAuthorityEpoch,
}

impl ProcessAuthority {
    pub(super) const fn request<'a>(
        &'a self,
        ids: &CommandIds,
        plan: &ExecutionPlan,
    ) -> ExecutionAuthorizationRequest<'a> {
        ExecutionAuthorizationRequest::new(
            &self.intent,
            &self.kernel,
            &self.capability,
            &self.budget,
            Some(&self.lease),
            &self.epoch,
            ids.revision,
            ids.session,
            ids.revision.workspace_generation(),
            ids.revision.workspace_revision(),
            instant(20),
            plan.digest(),
        )
    }
}

pub(super) fn commit_tool(
    path: &Path,
    ids: &CommandIds,
    contract: &AcceptanceContract,
    prepared: &PreparedToolCall,
    wall_millis: u64,
) -> Result<ToolAuthority, String> {
    let label = "tool-authority-store";
    let mut store = journal::open(path, ids, label)?;
    let intent = tool_action_intent(
        prepared,
        ids.actor,
        ActorRole::ProviderToolWorker,
        ids.environment,
        ids.resource,
    );
    let digest = intent
        .digest(CodecLimits::PRODUCTION)
        .map_err(|error| format!("digest command tool intent: {error}"))?;
    let capability_use = capability_use(ids, digest, OperationClass::Execution)?;
    let kernel =
        kernel::commit(&mut store, label, ids, contract, &intent, &capability_use, wall_millis)?;
    let capability = commit_capability(&mut store, label, ids, capability_use)?;
    let budget = commit_budget(&mut store, label, ids, digest, wall_millis)?;
    let epoch = allocate_epoch(&mut store)?;
    Ok(ToolAuthority { intent, kernel, capability, budget, epoch })
}

pub(super) fn commit_process(
    path: &Path,
    ids: &CommandIds,
    contract: &AcceptanceContract,
    plan: &ExecutionPlan,
    wall_millis: u64,
) -> Result<ProcessAuthority, String> {
    let label = "process-authority-store";
    let mut store = journal::open(path, ids, label)?;
    let intent = ActionIntentDto {
        action_id: ids.action,
        actor_id: ids.actor,
        role: ActorRole::ProviderToolWorker,
        environment_id: ids.environment,
        resource_id: ids.resource,
        capability_name: ids.capability.clone(),
        operation_class: OperationClass::RawEffect,
        media_type: EXECUTION_INTENT_MEDIA_TYPE.to_owned(),
        payload: ExecutionIntentPayload::new(
            ids.process,
            plan.digest(),
            plan.sandbox_digest(),
            plan.backend().descriptor_digest(),
        )
        .encode(),
    };
    let digest = intent
        .digest(CodecLimits::PRODUCTION)
        .map_err(|error| format!("digest command process intent: {error}"))?;
    let capability_use = capability_use(ids, digest, OperationClass::RawEffect)?;
    let kernel =
        kernel::commit(&mut store, label, ids, contract, &intent, &capability_use, wall_millis)?;
    let (capability, committed_lease) =
        lease::commit(&mut store, label, ids, capability_use, wall_millis)?;
    let budget = commit_budget(&mut store, label, ids, digest, wall_millis)?;
    let epoch = allocate_epoch(&mut store)?;
    Ok(ProcessAuthority { intent, kernel, capability, budget, lease: committed_lease, epoch })
}

pub(super) const fn instant(tick: u64) -> AuthorityInstant {
    AuthorityInstant::new(Generation::first(), tick)
}

fn capability_use(
    ids: &CommandIds,
    action_digest: Sha256Digest,
    operation_class: OperationClass,
) -> Result<CapabilityUseTransition, String> {
    let validity = ValidityWindow::new(instant(10), instant(1_000_000))
        .map_err(|error| format!("construct command capability validity: {error:?}"))?;
    let use_limit = UseLimit::limited(3)
        .map_err(|error| format!("construct command capability use limit: {error:?}"))?;
    let permissions =
        PermissionSet::new(vec![Permission::new(ids.resource, ids.capability.clone())])
            .map_err(|error| format!("construct command permission set: {error:?}"))?;
    let boundary_permissions =
        PermissionSet::new(vec![Permission::new(ids.resource, ids.capability.clone())])
            .map_err(|error| format!("construct command boundary permissions: {error:?}"))?;
    let scope = CapabilityScope::new(
        ids.actor,
        ActorRole::ProviderToolWorker,
        ids.environment,
        permissions,
        ids.revision,
        validity,
        use_limit,
    );
    let boundary = AuthorityBoundary::new(
        vec![ids.actor],
        vec![ActorRole::ProviderToolWorker],
        vec![ids.environment],
        boundary_permissions,
        ids.revision,
        validity,
        use_limit,
    )
    .map_err(|error| format!("construct command authority boundary: {error:?}"))?;
    let selector = ScopeSelector::new(
        ActorSelector::any_within_parent(),
        RoleSelector::any_within_parent(),
        EnvironmentSelector::any_within_parent(),
        PermissionSelector::any_within_parent(),
        ids.revision,
    );
    let ceiling = AuthorityCeiling::new(
        boundary,
        vec![CeilingGrant::new(
            super::contract::digest(ids.run, 0, "ceiling-grant"),
            selector,
            validity,
            use_limit,
        )],
        Vec::new(),
    )
    .map_err(|error| format!("construct command authority ceiling: {error:?}"))?;
    let risks = if operation_class == OperationClass::Execution {
        vec![RiskClass::Execution]
    } else {
        vec![RiskClass::ExternalSideEffect]
    };
    let operation = OperationDescriptor::new(
        ids.capability.clone(),
        operation_class,
        RiskSet::new(risks).map_err(|error| format!("construct command risks: {error:?}"))?,
    )
    .map_err(|error| format!("construct command operation: {error:?}"))?;
    let policy = PolicyDefinition::new(
        ids.revision.policy_id(),
        ceiling,
        OperationRegistry::new(vec![operation])
            .map_err(|error| format!("construct command operation registry: {error:?}"))?,
        Vec::new(),
    )
    .map_err(|error| format!("construct command policy: {error:?}"))?;
    let authorization = policy
        .evaluate(
            AuthorizationRequest::new(scope),
            AuthorityTimeState::new(instant(10)),
            instant(10),
        )
        .map_err(|error| format!("evaluate command policy: {error:?}"))?
        .into_parts()
        .0
        .ok_or_else(|| "command policy did not produce an authorization plan".to_owned())?;
    let capability = authorization
        .issue(
            ids.command("capability-issue-command")?,
            super::contract::digest(ids.run, 0, "capability-issue"),
        )
        .into_capability();
    capability
        .try_use(
            CapabilityUseRequest::new(
                ids.action,
                action_digest,
                Permission::new(ids.resource, ids.capability.clone()),
                ids.actor,
                ActorRole::ProviderToolWorker,
                ids.environment,
                ids.revision,
                instant(20),
            ),
            super::contract::digest(ids.run, 0, "capability-use"),
        )
        .map_err(|error| format!("use command capability: {error:?}"))
}

fn commit_capability(
    store: &mut SqliteJournal,
    store_label: &str,
    ids: &CommandIds,
    transition: CapabilityUseTransition,
) -> Result<CommittedCapabilityUse, String> {
    let key = AggregateKey::new(AggregateKind::Approval, ids.aggregate("capability")?);
    store
        .commit_capability_use(
            CapabilityCommitRequest::new(
                journal::append(
                    ids,
                    store_label,
                    key,
                    "capability-commit-command",
                    1,
                    "capability-commit-event",
                    None,
                    HeadExpectation::Absent(key),
                )?,
                transition,
                None,
            )
            .map_err(|error| format!("bind command capability transition: {error}"))?,
        )
        .map_err(|error| format!("commit command capability transition: {error}"))
}

fn commit_budget(
    store: &mut SqliteJournal,
    store_label: &str,
    ids: &CommandIds,
    action_digest: Sha256Digest,
    wall_millis: u64,
) -> Result<CommittedBudgetTransition, String> {
    let limits = BudgetLimits::new(BudgetAmounts::from_units(
        10,
        10,
        wall_millis.saturating_add(1_000),
        2,
        1,
    ));
    let ledger = BudgetLedger::new_root(ids.effect_budget, ids.revision, limits);
    let request = BudgetRequest::new(
        ids.reservation,
        ids.effect_budget,
        ids.revision,
        ids.action,
        action_digest,
        BudgetAmounts::from_units(0, 0, 0, 1, 0),
        BudgetAmounts::from_units(0, 0, wall_millis, 0, 0),
    );
    let transition = ledger
        .transition(BudgetCommand::Begin(request))
        .map_err(|error| format!("begin command effect budget: {error:?}"))?;
    let key = AggregateKey::new(AggregateKind::Budget, ids.aggregate("effect-budget")?);
    store
        .commit_budget_transition(
            BudgetCommitRequest::new(
                journal::append(
                    ids,
                    store_label,
                    key,
                    "budget-begin-command",
                    1,
                    "budget-begin-event",
                    None,
                    HeadExpectation::Absent(key),
                )?,
                transition,
                None,
                None,
            )
            .map_err(|error| format!("bind command budget transition: {error}"))?,
        )
        .map_err(|error| format!("commit command budget transition: {error}"))
}

fn allocate_epoch(store: &mut SqliteJournal) -> Result<CurrentAuthorityEpoch, String> {
    store
        .allocate_authority_epoch(ExpectedAuthorityEpoch::Absent)
        .map_err(|error| format!("allocate command authority epoch: {error}"))?;
    store
        .current_authority_epoch()
        .map_err(|error| format!("observe command authority epoch: {error}"))?
        .ok_or_else(|| "command authority epoch is missing".to_owned())
}
