//! Provider-independent identity and conversation binding.

use crate::ContextNodeId;
use peritus_role::HarnessRole;
use peritus_types::{RunId, WorkspaceId};
use vstd::prelude::*;

verus! {
/// One logical task view. Provider and invocation identities deliberately do not locate it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorkingBinding {
    run: RunId,
    workspace: WorkspaceId,
    task: ContextNodeId,
    role: HarnessRole,
    conversation_revision: u64,
}

impl WorkingBinding {
    /// Binds checked identities to a role and current conversation revision.
    #[must_use]
    pub const fn new(
        run: RunId,
        workspace: WorkspaceId,
        task: ContextNodeId,
        role: HarnessRole,
        conversation_revision: u64,
    ) -> Self {
        Self { run, workspace, task, role, conversation_revision }
    }
    /// Governing run.
    #[must_use]
    pub const fn run(self) -> RunId { self.run }
    /// Managed workspace identity.
    #[must_use]
    pub const fn workspace(self) -> WorkspaceId { self.workspace }
    /// Logical task within the run, stable across invocations.
    #[must_use]
    pub const fn task(self) -> ContextNodeId { self.task }
    /// Role that may read this working model.
    #[must_use]
    pub const fn role(self) -> HarnessRole { self.role }
    /// Incorporated public conversation revision.
    #[must_use]
    pub const fn conversation_revision(self) -> u64 { self.conversation_revision }

    /// Compares the stable scope without treating a later conversation as a new task.
    #[must_use]
    pub fn same_lineage(self, other: Self) -> bool {
        self.run == other.run && self.workspace == other.workspace
            && self.task == other.task && self.role == other.role
    }
}
}
