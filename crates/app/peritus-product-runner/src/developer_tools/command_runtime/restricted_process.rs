//! Trusted application-owned restricted children using the ordinary committed C2 authority.

use std::{collections::BTreeSet, path::PathBuf};

use peritus_process::{
    EnvironmentSource, EnvironmentValueSource, ExecutionAuthorizationRequest, ExecutionIdentity,
    ExecutionIsolation, ExecutionPlan,
};
use peritus_spec::AcceptanceContract;
use peritus_types::{RunId, Sha256Digest};

use super::{authority, contract, identity::CommandIds};
use crate::{ProductRunnerError, ProductRunnerErrorKind};

/// Inert exact identity for a restricted child of an already admitted host operation.
///
/// This is a trusted application-composition seam, not an approval endpoint. Before creating it,
/// the host must authenticate and admit its governing operation, validate frozen child inputs,
/// and durably link `source_run` and the nonzero child ordinal to that admitted operation. It must
/// not expose this constructor to model tool arguments or use it to authorize F0 promotion.
/// C2 independently consumes the committed exact plan before any operating-system execution.
pub struct RestrictedProcessContext {
    state_root: PathBuf,
    ids: CommandIds,
    contract: AcceptanceContract,
    allowed_environment: BTreeSet<String>,
}

impl RestrictedProcessContext {
    /// Creates an inert child context without committing permissions or touching the filesystem.
    ///
    /// # Errors
    /// Rejects a relative state root, zero ordinal, or invalid internally derived identities.
    pub fn new(
        state_root: impl Into<PathBuf>,
        source_run: RunId,
        ordinal: u64,
    ) -> Result<Self, ProductRunnerError> {
        let state_root = state_root.into();
        if !state_root.is_absolute() || ordinal == 0 {
            return Err(invalid(
                "restricted child requires an absolute protected state root and nonzero ordinal",
            ));
        }
        let contract = contract::command_contract(source_run, ordinal).map_err(invalid)?;
        let ids = CommandIds::new(source_run, ordinal, &contract).map_err(invalid)?;
        Ok(Self { state_root, ids, contract, allowed_environment: BTreeSet::new() })
    }

    /// Creates a restricted child whose cleared environment may contain exactly these literal
    /// host-admitted names. No ambient value is inherited and the final values remain bound into
    /// the exact execution-plan digest.
    pub(crate) fn new_with_cleared_environment(
        state_root: impl Into<PathBuf>,
        source_run: RunId,
        ordinal: u64,
        allowed_environment: impl IntoIterator<Item = String>,
    ) -> Result<Self, ProductRunnerError> {
        let mut context = Self::new(state_root, source_run, ordinal)?;
        context.allowed_environment = allowed_environment.into_iter().collect();
        Ok(context)
    }

    /// Complete nominal owner and revision required when preparing this exact child sandbox.
    #[must_use]
    pub const fn identity(&self) -> ExecutionIdentity {
        self.ids.execution_identity()
    }

    /// Commits real C0 dispatch, capability, budget, lease, and epoch receipts for one child.
    ///
    /// This operation grants only C2 execution of the supplied restricted native plan; it never
    /// creates a C4 tool invocation or approval/publication/promotion authority. Host admission
    /// of the governing operation remains the caller's mandatory boundary.
    ///
    /// # Errors
    /// Rejects identity drift, unrestricted/non-native execution, inherited credentials, missing
    /// finite limits, or failed durable commitment. All plan checks precede the first commitment.
    pub fn authorize(
        self,
        plan: &ExecutionPlan,
    ) -> Result<OwnedRestrictedProcessAuthority, ProductRunnerError> {
        validate(&self.ids, &self.allowed_environment, plan)?;
        let wall_millis = plan
            .resource_policy()
            .wall_millis()
            .ok_or_else(|| invalid("restricted child wall ceiling is absent"))?;
        let path = self
            .state_root
            .join("restricted-process-authority")
            .join(super::identity::action_hex(self.ids.action))
            .join("authority.sqlite3");
        let committed =
            authority::commit_process(&path, &self.ids, &self.contract, plan, wall_millis)
                .map_err(commit_error)?;
        Ok(OwnedRestrictedProcessAuthority {
            ids: self.ids,
            plan_digest: plan.digest(),
            allowed_environment: self.allowed_environment,
            committed,
        })
    }
}

/// Move-only real journal receipts for one application-owned restricted process plan.
pub struct OwnedRestrictedProcessAuthority {
    ids: CommandIds,
    plan_digest: Sha256Digest,
    allowed_environment: BTreeSet<String>,
    committed: authority::ProcessAuthority,
}

impl OwnedRestrictedProcessAuthority {
    /// Borrows receipts for C2's exact native execution gate.
    ///
    /// # Errors
    /// Rejects a different plan/identity instead of rebinding the committed child's authority.
    pub fn request(
        &self,
        plan: &ExecutionPlan,
    ) -> Result<ExecutionAuthorizationRequest<'_>, ProductRunnerError> {
        validate(&self.ids, &self.allowed_environment, plan)?;
        if self.plan_digest != plan.digest() {
            return Err(invalid(
                "restricted child plan differs from its committed exact authority",
            ));
        }
        Ok(self.committed.request(&self.ids, plan))
    }
}

fn validate(
    ids: &CommandIds,
    allowed_environment: &BTreeSet<String>,
    plan: &ExecutionPlan,
) -> Result<(), ProductRunnerError> {
    let resources = plan.resource_policy();
    let variables = plan.environment().variables();
    let environment_matches = variables.len() == allowed_environment.len()
        && variables.iter().all(|variable| {
            variable.source() == EnvironmentValueSource::Literal
                && allowed_environment.contains(variable.name())
        });
    if plan.identity() != ids.execution_identity()
        || plan.isolation() != ExecutionIsolation::Restricted
        || !plan.backend().is_native()
        || !matches!(plan.environment().source(), EnvironmentSource::Cleared)
        || !environment_matches
        || resources.wall_millis().is_none()
        || resources.cpu_millis().is_none()
        || resources.output_bytes().is_none()
        || plan.deadline_policy().wall_timeout_millis().is_none()
    {
        return Err(invalid(
            "restricted child identity, native isolation, credential denial, or finite bounds differ from the admitted host operation",
        ));
    }
    Ok(())
}

fn invalid(detail: impl Into<String>) -> ProductRunnerError {
    ProductRunnerError::new(
        ProductRunnerErrorKind::InvalidPrecondition,
        "plan restricted process authority",
        detail,
    )
}

fn commit_error(detail: impl Into<String>) -> ProductRunnerError {
    ProductRunnerError::new(
        ProductRunnerErrorKind::Apply,
        "commit restricted process authority",
        detail,
    )
}
