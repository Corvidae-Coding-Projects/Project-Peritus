//! Deterministically derived nominal identities for one product command action.

use peritus_journal::{AggregateId, StoreId};
use peritus_process::ExecutionIdentity;
use peritus_spec::AcceptanceContract;
use peritus_types::{
    ActionId, ActorId, AttemptId, BudgetId, BudgetReservationId, CapabilityName, CommandId,
    EnvironmentId, EventId, HarnessId, PolicyId, ProcessId, ProjectId, ProviderProfileId,
    ResourceId, RevisionNumber, RevisionTuple, RunId, SessionId, TurnId, WorkspaceId,
};
use sha2::{Digest as _, Sha256};

use super::contract;

pub(super) struct CommandIds {
    pub(super) workspace: WorkspaceId,
    pub(super) resource: ResourceId,
    pub(super) environment: EnvironmentId,
    pub(super) actor: ActorId,
    pub(super) session: SessionId,
    pub(super) action: ActionId,
    pub(super) process: ProcessId,
    pub(super) capability: CapabilityName,
    pub(super) revision: RevisionTuple,
    pub(super) project: ProjectId,
    pub(super) run: RunId,
    pub(super) attempt: AttemptId,
    pub(super) turn: TurnId,
    pub(super) kernel_root_budget: BudgetId,
    pub(super) kernel_child_budget: BudgetId,
    pub(super) effect_budget: BudgetId,
    pub(super) reservation: BudgetReservationId,
    source_run: RunId,
    ordinal: u64,
}

impl CommandIds {
    pub(super) fn new(
        source_run: RunId,
        ordinal: u64,
        acceptance: &AcceptanceContract,
    ) -> Result<Self, String> {
        let workspace = nominal(WorkspaceId::new, source_run, ordinal, "workspace")?;
        let revision = RevisionTuple::new(
            acceptance.id(),
            nominal(HarnessId::new, source_run, ordinal, "harness")?,
            workspace,
            peritus_types::Generation::first(),
            RevisionNumber::first(),
            nominal(PolicyId::new, source_run, ordinal, "policy")?,
            nominal(ProviderProfileId::new, source_run, ordinal, "provider")?,
        );
        Ok(Self {
            workspace,
            resource: nominal(ResourceId::new, source_run, ordinal, "resource")?,
            environment: nominal(EnvironmentId::new, source_run, ordinal, "environment")?,
            actor: nominal(ActorId::new, source_run, ordinal, "actor")?,
            session: nominal(SessionId::new, source_run, ordinal, "session")?,
            action: nominal(ActionId::new, source_run, ordinal, "action")?,
            process: nominal(ProcessId::new, source_run, ordinal, "process")?,
            capability: CapabilityName::new("shell.exec".to_owned())
                .map_err(|error| format!("construct command capability: {error:?}"))?,
            revision,
            project: nominal(ProjectId::new, source_run, ordinal, "project")?,
            run: nominal(RunId::new, source_run, ordinal, "execution-run")?,
            attempt: nominal(AttemptId::new, source_run, ordinal, "attempt")?,
            turn: nominal(TurnId::new, source_run, ordinal, "turn")?,
            kernel_root_budget: nominal(BudgetId::new, source_run, ordinal, "kernel-root-budget")?,
            kernel_child_budget: nominal(
                BudgetId::new,
                source_run,
                ordinal,
                "kernel-child-budget",
            )?,
            effect_budget: nominal(BudgetId::new, source_run, ordinal, "effect-budget")?,
            reservation: nominal(BudgetReservationId::new, source_run, ordinal, "reservation")?,
            source_run,
            ordinal,
        })
    }

    pub(super) const fn execution_identity(&self) -> ExecutionIdentity {
        ExecutionIdentity::new(
            self.project,
            self.session,
            self.run,
            self.attempt,
            self.turn,
            self.action,
            self.process,
            self.workspace,
            self.resource,
            self.environment,
            self.actor,
            self.revision,
        )
    }

    pub(super) fn command(&self, label: &str) -> Result<CommandId, String> {
        nominal(CommandId::new, self.source_run, self.ordinal, label)
    }

    pub(super) fn event(&self, label: &str) -> Result<EventId, String> {
        nominal(EventId::new, self.source_run, self.ordinal, label)
    }

    pub(super) fn aggregate(&self, label: &str) -> Result<AggregateId, String> {
        nominal(AggregateId::new, self.source_run, self.ordinal, label)
    }

    pub(super) fn store(&self, label: &str) -> Result<StoreId, String> {
        nominal(StoreId::new, self.source_run, self.ordinal, label)
    }
}

fn nominal<T, E>(
    constructor: impl FnOnce([u8; 16]) -> Result<T, E>,
    run_id: RunId,
    ordinal: u64,
    label: &str,
) -> Result<T, String>
where
    E: std::fmt::Debug,
{
    constructor(contract::id(run_id, ordinal, label))
        .map_err(|error| format!("construct product command identity {label}: {error:?}"))
}

pub(super) fn bounded_key(value: &str) -> String {
    hex(&Sha256::digest(value.as_bytes()))
}

pub(super) fn action_hex(action: ActionId) -> String {
    hex(action.as_bytes())
}

fn hex(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len().saturating_mul(2));
    for byte in bytes {
        use core::fmt::Write as _;
        let _ = write!(output, "{byte:02x}");
    }
    output
}
