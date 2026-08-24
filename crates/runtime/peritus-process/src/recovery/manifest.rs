//! Canonical durable process ownership manifest.

use peritus_leases::LeaseClaim;
use peritus_types::{
    ActorId, EnvironmentId, Generation, ResourceId, RevisionNumber, SessionId, Sha256Digest,
    WorkspaceId,
};

use crate::{
    ExecutionIdentity, ExecutionPlan, LifecyclePhase, OsExitObservation, ProcessError, StopTrigger,
    TerminalResult, WorkspaceAccess, platform::ProcessTreeIdentity,
};

mod codec;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct LeaseOwnership {
    workspace_id: WorkspaceId,
    resource_id: ResourceId,
    environment_id: EnvironmentId,
    actor_id: ActorId,
    session_id: SessionId,
    generation: Generation,
    claim_version: RevisionNumber,
}

impl LeaseOwnership {
    pub(crate) const fn from_claim(claim: LeaseClaim) -> Self {
        let scope = claim.scope();
        let holder = claim.holder();
        Self {
            workspace_id: scope.workspace_id(),
            resource_id: scope.resource_id(),
            environment_id: scope.environment_id(),
            actor_id: holder.actor_id(),
            session_id: holder.session_id(),
            generation: claim.generation(),
            claim_version: claim.claim_version(),
        }
    }

    pub(crate) fn matches_claim(self, claim: LeaseClaim) -> bool {
        let other = Self::from_claim(claim);
        self == other
    }

    pub(crate) const fn workspace_id(self) -> WorkspaceId {
        self.workspace_id
    }
    pub(crate) const fn resource_id(self) -> ResourceId {
        self.resource_id
    }
    pub(crate) const fn environment_id(self) -> EnvironmentId {
        self.environment_id
    }
    pub(crate) const fn actor_id(self) -> ActorId {
        self.actor_id
    }
    pub(crate) const fn session_id(self) -> SessionId {
        self.session_id
    }
    pub(crate) const fn generation(self) -> Generation {
        self.generation
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ExecutionManifest {
    pub(crate) identity: ExecutionIdentity,
    pub(crate) action_digest: Sha256Digest,
    pub(crate) plan_digest: Sha256Digest,
    pub(crate) sandbox_digest: Sha256Digest,
    pub(crate) backend_digest: Sha256Digest,
    pub(crate) support_digest: Sha256Digest,
    pub(crate) preparation_digest: Sha256Digest,
    pub(crate) access: WorkspaceAccess,
    pub(crate) lease: Option<LeaseOwnership>,
    pub(crate) phase: LifecyclePhase,
    pub(crate) tree: Option<ProcessTreeIdentity>,
    pub(crate) trigger: Option<StopTrigger>,
    pub(crate) exit: Option<OsExitObservation>,
    pub(crate) observed_output: u64,
    pub(crate) retained_output: u64,
    pub(crate) dropped_output: u64,
    pub(crate) tree_quiescent: bool,
    pub(crate) support_tasks_joined: bool,
    pub(crate) terminal_digest: Option<Sha256Digest>,
    pub(crate) terminal: Option<TerminalResult>,
}

impl ExecutionManifest {
    pub(crate) fn authorized(
        plan: &ExecutionPlan,
        action_digest: Sha256Digest,
        lease: Option<LeaseClaim>,
    ) -> Self {
        Self {
            identity: plan.identity(),
            action_digest,
            plan_digest: plan.digest(),
            sandbox_digest: plan.sandbox_digest(),
            backend_digest: plan.backend().descriptor_digest(),
            support_digest: plan.backend().support_digest(),
            preparation_digest: plan.backend().preparation_digest(),
            access: plan.working_directory().access(),
            lease: lease.map(LeaseOwnership::from_claim),
            phase: LifecyclePhase::Authorized,
            tree: None,
            trigger: None,
            exit: None,
            observed_output: 0,
            retained_output: 0,
            dropped_output: 0,
            tree_quiescent: false,
            support_tasks_joined: false,
            terminal_digest: None,
            terminal: None,
        }
    }

    pub(crate) fn matches_terminal(&self, result: &TerminalResult) -> bool {
        let Some((observed, retained, dropped)) =
            result.output().streams().iter().try_fold((0_u64, 0_u64, 0_u64), |totals, stream| {
                Some((
                    totals.0.checked_add(stream.observed())?,
                    totals.1.checked_add(stream.retained())?,
                    totals.2.checked_add(stream.dropped())?,
                ))
            })
        else {
            return false;
        };
        result.process_id() == self.identity.process_id()
            && result.plan_digest() == self.plan_digest
            && self.exit.as_ref() == Some(result.os_exit())
            && self.trigger == result.first_trigger()
            && self.observed_output == observed
            && self.retained_output == retained
            && self.dropped_output == dropped
            && self.tree_quiescent == result.tree_cleanup_complete()
            && self.support_tasks_joined == result.support_tasks_joined()
    }

    pub(crate) fn encode(&self) -> Result<Vec<u8>, ProcessError> {
        codec::encode(self)
    }

    pub(crate) fn decode(bytes: &[u8]) -> Result<Self, ProcessError> {
        codec::decode(bytes)
    }
}
