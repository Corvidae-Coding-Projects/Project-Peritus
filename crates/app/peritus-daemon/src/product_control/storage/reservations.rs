//! Child identities reserved by durable prepared restores under the single C0 owner.

use super::{ControlError, ControlOperation, ControlStore, ConversationId, Error};
use peritus_product_runner::control::{ControlIntent, RestoreStatus};

impl ControlStore {
    pub(super) fn check_creation_reservation(
        &self,
        operation: &ControlOperation,
        creating: bool,
    ) -> Result<(), Error> {
        if creating {
            self.check_reserved_child(operation.conversation(), None)?;
        }
        if let ControlIntent::PrepareRestore { restore, .. } = operation.intent()
            && let Some(branch) = restore.branch()
        {
            if self.load(branch.child())?.is_some() {
                return Err(ControlError::IdempotencyConflict.into());
            }
            self.check_reserved_child(branch.child(), None)?;
        }
        Ok(())
    }

    pub(super) fn check_reserved_child(
        &self,
        child: ConversationId,
        publication: Option<&ControlOperation>,
    ) -> Result<(), Error> {
        // The bounded catalog is rebuilt from authoritative events. The same exclusive store
        // owner holds this check through append, so preparing the restore atomically reserves
        // its child ID without publishing a child or introducing a second mutable index.
        for source in self.conversation_ids()?.keys() {
            let record = self.load(*source)?.ok_or(Error::Corrupt("catalog source missing"))?;
            for restore in record.restores() {
                let Some(branch) = restore.branch() else { continue };
                if branch.child() != child || restore.status() == RestoreStatus::Conflict {
                    continue;
                }
                let exact = publication.is_some_and(|operation| {
                    operation.conversation() == *source
                        && operation.actor_bytes() == record.owner_bytes()
                        && matches!(operation.intent(), ControlIntent::PublishRestoreBranch {
                            restore: selected, branch: selected_branch,
                        } if *selected == restore.id() && selected_branch == branch)
                });
                if !exact {
                    return Err(ControlError::IdempotencyConflict.into());
                }
            }
        }
        Ok(())
    }
}
