//! Atomic workspace restriction sidecar publication in the existing control journal.

use super::{ControlStore, Error};
use peritus_journal::StateInstall;
use peritus_product_runner::control::{
    ControlError, ControlIntent, ControlOperation, HostPermissions, PermissionPolicy,
};
use peritus_types::WorkspaceId;

pub(super) const PERMISSION_NAMESPACE: u16 = 3540;

impl ControlStore {
    /// Loads the workspace-scoped user restriction overlay without creating state.
    pub(crate) fn permission_policy(
        &self,
        workspace: WorkspaceId,
    ) -> Result<PermissionPolicy, Error> {
        self.journal.state_record(PERMISSION_NAMESPACE, workspace.as_bytes())?.map_or_else(
            || Ok(PermissionPolicy::default()),
            |record| {
                let policy = PermissionPolicy::parse(record.bytes())?;
                if policy.revision() != record.revision() {
                    return Err(Error::Corrupt("permission revision differs from C0 state"));
                }
                Ok(policy)
            },
        )
    }

    /// Atomically advances the conversation fence, original receipt, and workspace restriction.
    pub(crate) fn accept_permissions(
        &mut self,
        operation: &ControlOperation,
        host: HostPermissions,
    ) -> Result<peritus_product_runner::control::ControlReceipt, Error> {
        if let Some(receipt) = self.resolve(operation)? {
            return Ok(receipt);
        }
        let ControlIntent::SetPermissions { expected_policy_revision, capability, allowed } =
            operation.intent()
        else {
            return Err(ControlError::InvalidInput.into());
        };
        let current = self.permission_policy(
            WorkspaceId::new(*operation.workspace_bytes())
                .map_err(|_| ControlError::InvalidInput)?,
        )?;
        let next = current.apply(*expected_policy_revision, *capability, *allowed, host)?;
        let expected = (current.revision() != 0).then_some(current.revision());
        let install = StateInstall::new(
            PERMISSION_NAMESPACE,
            operation.workspace_bytes().to_vec(),
            expected,
            next.revision(),
            next.canonical_bytes()?,
        )?;
        self.accept_installs(operation, vec![install])
    }
}
