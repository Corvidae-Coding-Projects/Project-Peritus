//! Restricted native execution seam for already-admitted host-owned module jobs.

use peritus_process::{ExecutionPlan, OwnedProcess};
use peritus_sandbox::{BackendAdmission, CheckedSandboxPlan};
use peritus_sandbox_linux::LinuxBackend;
use peritus_types::RunId;

use super::{CommandRuntime, ordinal};
use crate::{ProductRunnerError, RestrictedProcessContext};

impl CommandRuntime {
    pub(crate) fn reserve_native_module_job_context_with_environment(
        &self,
        source_run: RunId,
        allowed_environment: impl IntoIterator<Item = String>,
    ) -> Result<RestrictedProcessContext, ProductRunnerError> {
        let mut state = self.inner.state.lock().map_err(|_| {
            ProductRunnerError::new(
                crate::ProductRunnerErrorKind::Apply,
                "reserve native module job",
                "command runtime is poisoned",
            )
        })?;
        let ordinal = ordinal::reserve(&self.inner.state_root, source_run, state.next_ordinal)
            .map_err(|error| {
                ProductRunnerError::new(
                    crate::ProductRunnerErrorKind::Apply,
                    "reserve native module job",
                    error,
                )
            })?;
        state.next_ordinal = ordinal;
        drop(state);
        RestrictedProcessContext::new_with_cleared_environment(
            &self.inner.state_root,
            source_run,
            ordinal,
            allowed_environment,
        )
    }

    /// Commits exact C2 authority and launches one already-validated native module job.
    ///
    /// This is intentionally crate-private. Module payloads cannot construct the authority
    /// context, execution plan, checked sandbox, admission, or backend passed through this seam.
    pub(crate) fn launch_native_module_job(
        &self,
        context: RestrictedProcessContext,
        plan: ExecutionPlan,
        sandbox: &CheckedSandboxPlan,
        admission: &BackendAdmission,
        backend: LinuxBackend,
    ) -> Result<OwnedProcess, ProductRunnerError> {
        let authority = context.authorize(&plan)?;
        let request = authority.request(&plan)?;
        self.inner.gateway.launch_with_backend(&request, plan, sandbox, admission, backend).map_err(
            |error| {
                ProductRunnerError::new(
                    crate::ProductRunnerErrorKind::Apply,
                    "launch native module job",
                    error.to_string(),
                )
            },
        )
    }
}
