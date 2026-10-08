//! Local working-memory host: C0 effects around C6 deterministic state.

mod assembly;
mod checkpoint_validation;
mod driver;
mod grounding;
mod memory;
mod port;
mod record;
mod reentry;
mod semantic;
mod storage;
#[cfg(test)]
mod tests;
mod tools;
mod view_binding;

pub use driver::{InvocationAccounting, run_live_invocation};
pub(crate) use driver::run_live_invocation_with_receipts;
pub use port::LocalContextHandle;
pub use storage::LocalContextInspection;
pub use tools::MemoryTools;

pub use crate::context_config::{LocalContextConfig, LocalProcessConfig, LocalSemanticBackend};

use peritus_agent::DeveloperLoopError;

fn error(operation: &'static str) -> DeveloperLoopError {
    DeveloperLoopError::Context(format!("local working memory: {operation}"))
}

/// Inspects the exact last published view without recovery, workspace access, or journal writes.
/// The JSON includes source references, checkpoint validation, and the exact C5 message archive.
///
/// # Errors
/// Rejects absent checkpoints, cross-lineage identities, corrupt artifacts, and allocation failure.
pub fn inspect_local_context(
    trace: &std::path::Path,
    run: peritus_types::RunId,
    workspace: peritus_types::WorkspaceId,
    role: peritus_role::HarnessRole,
) -> Result<String, DeveloperLoopError> {
    let name = match role {
        peritus_role::HarnessRole::Writer => "writer",
        peritus_role::HarnessRole::Fixer => "fixer",
        peritus_role::HarnessRole::Reviewer => "reviewer",
        _ => return Err(error("unsupported inspection role")),
    };
    let task = peritus_context::ContextNodeId::new(run.into_bytes())
        .map_err(|_| error("invalid inspection task identity"))?;
    let binding = peritus_context::working::WorkingBinding::new(run, workspace, task, role, 0);
    storage::inspect(&trace.with_extension("context").join(name), binding)
}

/// Opens one exact published frontier with paged records and canonical archive reads.
///
/// # Errors
/// Rejects unsupported roles, absent checkpoints, conflicting lineage, and corrupt evidence.
pub fn open_local_context_inspection(
    trace: &std::path::Path,
    run: peritus_types::RunId,
    workspace: peritus_types::WorkspaceId,
    role: peritus_role::HarnessRole,
) -> Result<LocalContextInspection, DeveloperLoopError> {
    let name = match role {
        peritus_role::HarnessRole::Writer => "writer",
        peritus_role::HarnessRole::Fixer => "fixer",
        peritus_role::HarnessRole::Reviewer => "reviewer",
        _ => return Err(error("unsupported inspection role")),
    };
    let task = peritus_context::ContextNodeId::new(run.into_bytes())
        .map_err(|_| error("invalid inspection task identity"))?;
    let binding = peritus_context::working::WorkingBinding::new(run, workspace, task, role, 0);
    storage::open_inspection(&trace.with_extension("context").join(name), binding)
}
