//! Fresh production lookup. Captured bindings remain immutable across later activations.

use super::{evaluation as e, local::LocalProgram};
use crate::DaemonError;
use peritus_artifact_store::{ArtifactDigest, ArtifactStore};
use peritus_evolution::ProductionHarnessBinding;
use peritus_harness::domain::ComponentId;
use peritus_journal::SqliteJournal;
use peritus_types::ProjectId;

/// Verified immutable executable reference captured from the current production pointer.
/// This is executable input data, not process or promotion authority.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProductionProgram {
    binding: ProductionHarnessBinding,
    artifact: ArtifactDigest,
    bytes: u64,
}
impl ProductionProgram {
    /// Governing E1 binding captured at lookup, unchanged by subsequent pointer updates.
    #[must_use]
    pub const fn binding(self) -> ProductionHarnessBinding {
        self.binding
    }
    /// Exact finalized executable root selected by that materialized E1 component.
    #[must_use]
    pub const fn artifact(self) -> ArtifactDigest {
        self.artifact
    }
    /// Verified actual byte length.
    #[must_use]
    pub const fn bytes(self) -> u64 {
        self.bytes
    }
    /// Builds inert input for a later explicitly admitted restricted local operation.
    pub fn local_program(self, arguments: Vec<String>) -> Result<LocalProgram, DaemonError> {
        LocalProgram::new(self.artifact, self.bytes, arguments, Vec::new()).map_err(e::error)
    }
}

/// Resolves the current authoritative pointer and exact retained materialization on every call.
///
/// # Errors
/// Rejects absent production, missing component/executable, stale materialization or artifact drift.
pub fn current_program(
    journal: &SqliteJournal,
    artifacts: &ArtifactStore,
    project: ProjectId,
    component: &ComponentId,
) -> Result<ProductionProgram, DaemonError> {
    let (revision, binding) =
        super::evolution::resolve_current_binding(journal, artifacts, project).map_err(e::error)?;
    let declared = revision
        .graph()
        .declaration(component)
        .ok_or_else(|| e::invalid("production component is absent"))?;
    let artifact = declared
        .executable_artifact_digest()
        .ok_or_else(|| e::invalid("production component has no executable artifact"))?;
    let artifact = ArtifactDigest::new(*artifact.as_bytes());
    let bytes = artifacts.verify(artifact).map_err(e::error)?.size();
    if bytes == 0 {
        return Err(e::invalid("production executable artifact is empty"));
    }
    Ok(ProductionProgram { binding, artifact, bytes })
}
