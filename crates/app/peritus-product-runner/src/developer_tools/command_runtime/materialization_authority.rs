//! Exact host-owned C1 authority for an actually committed E1 materialization.
//!
//! This seam cannot publish, approve, or activate F0 candidates. The authenticated host owns the
//! admitted parent operation and actual registered C1 target; the supplied opaque `CommittedPlan`
//! proves that E1 has committed its exact materialization before these effect grants are callable.

use std::path::PathBuf;

use peritus_artifact_store::ArtifactStore;
use peritus_harness::{
    CommittedPlan, MaterializationAuthorizationPayloads, materialization::AuthorizationActions,
    materialization_authorization_payloads,
};
use peritus_journal::{
    CommittedCapabilityUse, CommittedKernelTransition, CommittedLeaseTransition,
    CurrentAuthorityEpoch,
};
use peritus_leases::LeaseHolder;
use peritus_policy::{ActorRole, OperationClass, RiskClass};
use peritus_protocol::ActionIntentDto;
use peritus_types::{
    ActorId, CapabilityName, EnvironmentId, ResourceId, RevisionTuple, RunId, SessionId,
    Sha256Digest, SnapshotId,
};
use peritus_workspace::WorkspaceAuthorizationRequest;

use super::{authority, contract, identity::CommandIds, journal, kernel, lease};
use crate::{ProductRunnerError, ProductRunnerErrorKind};

const WINDOW_MILLIS: u64 = 60_000;

/// Actual target and authenticated caller for one already committed materialization.
///
/// The host must admit the parent E1 operation, authenticate the actor/session, and resolve these
/// resource/environment identities from its registered C1 target. This is not a model-tool input.
pub struct MaterializationAuthorityRequest<'a> {
    committed: &'a CommittedPlan,
    artifacts: &'a ArtifactStore,
    actor: ActorId,
    session: SessionId,
    resource: ResourceId,
    environment: EnvironmentId,
    actions: AuthorizationActions,
    snapshot: SnapshotId,
}

impl<'a> MaterializationAuthorityRequest<'a> {
    /// Retains the exact actual C0 plan and target-owned caller identities without granting effects.
    #[must_use]
    #[allow(
        clippy::too_many_arguments,
        reason = "authenticated caller and exact target remain explicit"
    )]
    pub const fn new(
        committed: &'a CommittedPlan,
        artifacts: &'a ArtifactStore,
        actor: ActorId,
        session: SessionId,
        resource: ResourceId,
        environment: EnvironmentId,
        actions: AuthorizationActions,
        snapshot: SnapshotId,
    ) -> Self {
        Self { committed, artifacts, actor, session, resource, environment, actions, snapshot }
    }
}

/// Inert child identity of a host-admitted E1 materialization.
pub struct MaterializationAuthorityContext {
    state_root: PathBuf,
    parent: RunId,
    ordinal: u64,
}

impl MaterializationAuthorityContext {
    /// Retains an absolute protected authority root and unique durable parent-child ordinal.
    ///
    /// # Errors
    /// Rejects relative roots, zero ordinal, or child identity overflow.
    pub fn new(
        state_root: impl Into<PathBuf>,
        parent: RunId,
        ordinal: u64,
    ) -> Result<Self, ProductRunnerError> {
        let state_root = state_root.into();
        if !state_root.is_absolute()
            || ordinal == 0
            || ordinal.checked_mul(2).and_then(|value| value.checked_add(1)).is_none()
        {
            return Err(invalid(
                "materialization authority requires an absolute protected root and bounded nonzero ordinal",
            ));
        }
        Ok(Self { state_root, parent, ordinal })
    }

    /// Commits separate actual B0 dispatch, B1 capability/lease, and epoch receipts for E1 effects.
    ///
    /// Both payloads are reconstructed from the actual committed plan and verified component
    /// artifacts. Arbitrary caller payload bytes cannot substitute for these exact effects.
    ///
    /// # Errors
    /// Rejects missing/corrupt actual artifacts, wrong action identities, or any durable commitment.
    pub fn authorize(
        &self,
        request: &MaterializationAuthorityRequest<'_>,
    ) -> Result<OwnedMaterializationAuthority, ProductRunnerError> {
        if request.actions.patch() == request.actions.candidate() {
            return Err(invalid("materialization patch and candidate actions must be distinct"));
        }
        let payloads = materialization_authorization_payloads(
            request.committed.plan(),
            request.artifacts,
            request.resource,
            request.actions,
            request.snapshot,
        )
        .map_err(|error| invalid(&error.to_string()))?;
        let first = self
            .ordinal
            .checked_mul(2)
            .ok_or_else(|| invalid("materialization ordinal overflowed"))?;
        let patch = self.commit(request, &payloads, first, false)?;
        let candidate = self.commit(
            request,
            &payloads,
            first.checked_add(1).ok_or_else(|| invalid("materialization ordinal overflowed"))?,
            true,
        )?;
        Ok(OwnedMaterializationAuthority {
            patch,
            candidate,
            plan_digest: request.committed.plan().digest(),
            actions: request.actions,
        })
    }

    fn commit(
        &self,
        request: &MaterializationAuthorityRequest<'_>,
        payloads: &MaterializationAuthorizationPayloads,
        ordinal: u64,
        candidate: bool,
    ) -> Result<C1Authority, ProductRunnerError> {
        let contract = contract::command_contract(self.parent, ordinal).map_err(commit_error)?;
        let mut ids = CommandIds::new(self.parent, ordinal, &contract).map_err(commit_error)?;
        let target = request.committed.plan().target();
        ids.workspace = target.workspace_id();
        ids.resource = request.resource;
        ids.environment = request.environment;
        ids.actor = request.actor;
        ids.session = request.session;
        ids.action = if candidate { request.actions.candidate() } else { request.actions.patch() };
        ids.revision = RevisionTuple::new(
            contract.id(),
            request.committed.plan().harness_id(),
            target.workspace_id(),
            target.generation(),
            target.revision(),
            ids.revision.policy_id(),
            ids.revision.provider_profile_id(),
        );
        let capability = if candidate { "workspace.candidate" } else { "workspace.patch" };
        ids.capability = CapabilityName::new(capability.to_owned())
            .map_err(|_| invalid("materialization capability is invalid"))?;
        let intent = ActionIntentDto {
            action_id: ids.action,
            actor_id: ids.actor,
            role: ActorRole::ProviderToolWorker,
            environment_id: ids.environment,
            resource_id: ids.resource,
            capability_name: ids.capability.clone(),
            operation_class: OperationClass::WorkspaceMutation,
            media_type: "application/vnd.peritus.workspace-operation+binary".to_owned(),
            payload: if candidate {
                payloads.candidate().to_vec()
            } else {
                payloads.patch().to_vec()
            },
        };
        let digest = intent
            .digest(peritus_codec::CodecLimits::PRODUCTION)
            .map_err(|error| commit_error(error.to_string()))?;
        let capability = authority::capability_use(
            &ids,
            digest,
            OperationClass::WorkspaceMutation,
            RiskClass::ScopedWrite,
        )
        .map_err(commit_error)?;
        let path = self
            .state_root
            .join("materialization-authority")
            .join(super::identity::action_hex(ids.action))
            .join("authority.sqlite3");
        let label = "materialization-authority-store";
        let mut store = journal::open(&path, &ids, label).map_err(commit_error)?;
        let kernel =
            kernel::commit(&mut store, label, &ids, &contract, &intent, &capability, WINDOW_MILLIS)
                .map_err(commit_error)?;
        let (capability, lease) = lease::commit(&mut store, label, &ids, capability, WINDOW_MILLIS)
            .map_err(commit_error)?;
        let epoch = authority::allocate_epoch(&mut store).map_err(commit_error)?;
        Ok(C1Authority {
            intent,
            kernel,
            capability,
            lease,
            epoch,
            revision: ids.revision,
            session: ids.session,
            contract,
        })
    }
}

struct C1Authority {
    intent: ActionIntentDto,
    kernel: CommittedKernelTransition,
    capability: CommittedCapabilityUse,
    lease: CommittedLeaseTransition,
    epoch: CurrentAuthorityEpoch,
    revision: RevisionTuple,
    session: SessionId,
    contract: peritus_spec::AcceptanceContract,
}

impl C1Authority {
    const fn request(&self) -> WorkspaceAuthorizationRequest<'_> {
        WorkspaceAuthorizationRequest::new(
            &self.intent,
            &self.kernel,
            &self.capability,
            &self.lease,
            &self.epoch,
            self.revision,
            self.session,
            self.revision.workspace_generation(),
            self.revision.workspace_revision(),
            authority::instant(20),
        )
    }
}

/// Move-only actual effect authority for the exact E1 materialization.
pub struct OwnedMaterializationAuthority {
    patch: C1Authority,
    candidate: C1Authority,
    plan_digest: Sha256Digest,
    actions: AuthorizationActions,
}

impl OwnedMaterializationAuthority {
    /// Borrows the actual candidate action contract already checked by the committed B0 owner.
    #[must_use]
    pub const fn contract(&self) -> &peritus_spec::AcceptanceContract {
        &self.candidate.contract
    }
    /// Borrows the exact lower patch request only for this actual committed plan.
    ///
    /// # Errors
    /// Rejects another committed materialization identity.
    pub fn patch_request(
        &self,
        committed: &CommittedPlan,
    ) -> Result<WorkspaceAuthorizationRequest<'_>, ProductRunnerError> {
        self.require_plan(committed)?;
        Ok(self.patch.request())
    }
    /// Borrows the separate exact lower candidate request only for this actual committed plan.
    ///
    /// # Errors
    /// Rejects another committed materialization identity.
    pub fn candidate_request(
        &self,
        committed: &CommittedPlan,
    ) -> Result<WorkspaceAuthorizationRequest<'_>, ProductRunnerError> {
        self.require_plan(committed)?;
        Ok(self.candidate.request())
    }
    /// Returns the complete candidate action revision tuple for the resulting governing binding.
    #[must_use]
    pub const fn revision(&self) -> RevisionTuple {
        self.candidate.revision
    }
    /// Returns the actual authenticated actor/session holder required by C1.
    #[must_use]
    pub const fn lease_holder(&self) -> LeaseHolder {
        LeaseHolder::new(self.candidate.intent.actor_id, self.candidate.session)
    }
    /// Returns the two exact effect action identities.
    #[must_use]
    pub const fn actions(&self) -> AuthorizationActions {
        self.actions
    }
    fn require_plan(&self, committed: &CommittedPlan) -> Result<(), ProductRunnerError> {
        if committed.plan().digest() != self.plan_digest {
            return Err(invalid("materialization authority belongs to another committed plan"));
        }
        Ok(())
    }
}

fn invalid(detail: &str) -> ProductRunnerError {
    ProductRunnerError::new(
        ProductRunnerErrorKind::InvalidPrecondition,
        "authorize actual E1 materialization",
        detail,
    )
}
fn commit_error(detail: String) -> ProductRunnerError {
    ProductRunnerError::new(
        ProductRunnerErrorKind::InternalInvariant,
        "commit actual E1 materialization authority",
        detail,
    )
}
