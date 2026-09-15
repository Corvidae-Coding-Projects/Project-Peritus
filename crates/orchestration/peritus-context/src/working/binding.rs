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
    /// Logical view of the governing run.
    pub closed spec fn spec_run(self) -> RunId { self.run }
    /// Logical view of the managed workspace.
    pub closed spec fn spec_workspace(self) -> WorkspaceId { self.workspace }
    /// Logical view of the stable task identity.
    pub closed spec fn spec_task(self) -> ContextNodeId { self.task }
    /// Logical view of the role-scoped model.
    pub closed spec fn spec_role(self) -> HarnessRole { self.role }
    /// Logical view of the incorporated conversation revision.
    pub closed spec fn spec_conversation_revision(self) -> u64 { self.conversation_revision }
    /// Exact stable scope comparison, excluding only the advancing conversation revision.
    pub open spec fn spec_same_lineage(self, other: Self) -> bool {
        self.spec_run().spec_bytes() == other.spec_run().spec_bytes()
            && self.spec_workspace().spec_bytes() == other.spec_workspace().spec_bytes()
            && self.spec_task().spec_matches(&other.spec_task())
            && self.spec_role() == other.spec_role()
    }

    /// Binds checked identities to a role and current conversation revision.
    #[must_use]
    pub const fn new(
        run: RunId,
        workspace: WorkspaceId,
        task: ContextNodeId,
        role: HarnessRole,
        conversation_revision: u64,
    ) -> (result: Self)
        ensures
            result.spec_run() == run,
            result.spec_workspace() == workspace,
            result.spec_task() == task,
            result.spec_role() == role,
            result.spec_conversation_revision() == conversation_revision,
    {
        Self { run, workspace, task, role, conversation_revision }
    }
    /// Governing run.
    #[must_use]
    pub const fn run(self) -> (result: RunId)
        ensures result == self.spec_run(),
    { self.run }
    /// Managed workspace identity.
    #[must_use]
    pub const fn workspace(self) -> (result: WorkspaceId)
        ensures result == self.spec_workspace(),
    { self.workspace }
    /// Logical task within the run, stable across invocations.
    #[must_use]
    pub const fn task(self) -> (result: ContextNodeId)
        ensures result == self.spec_task(),
    { self.task }
    /// Role that may read this working model.
    #[must_use]
    pub const fn role(self) -> (result: HarnessRole)
        ensures result == self.spec_role(),
    { self.role }
    /// Incorporated public conversation revision.
    #[must_use]
    pub const fn conversation_revision(self) -> (result: u64)
        ensures result == self.spec_conversation_revision(),
    { self.conversation_revision }

    /// Compares the stable scope without treating a later conversation as a new task.
    #[must_use]
    pub fn same_lineage(self, other: Self) -> bool {
        self.run == other.run && self.workspace == other.workspace
            && self.task == other.task && self.role == other.role
    }
}
}
