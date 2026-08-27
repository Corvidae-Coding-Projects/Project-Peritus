//! Single writable-state authority owner and bounded client facade.
//!
//! Construction remains separate from the client surface and serialized message loop.

mod error;
mod handle;
mod runtime;
mod storage;

use peritus_artifact_store::ArtifactStore;
use peritus_journal::SqliteJournal;
use tokio::{sync::mpsc, task::JoinHandle};

pub use handle::AuthorityHandle;

use crate::{
    DaemonError, DaemonErrorCode, DaemonLifecycle, DaemonRecovery,
    artifact::ArtifactAuthority,
    prompt::{PromptBroker, PromptBrokerLimits},
};

/// Namespace for starting the single writable-state owner task.
#[derive(Clone, Copy, Debug, Default)]
pub struct AuthorityOwner;

impl AuthorityOwner {
    /// Starts one bounded serialized owner on the current Tokio runtime.
    ///
    /// # Errors
    ///
    /// Returns invalid input for a zero or excessive queue capacity.
    pub fn spawn(
        journal: SqliteJournal,
        lifecycle: DaemonLifecycle,
        artifacts: ArtifactStore,
        maximum_artifact_bytes: u64,
        maximum_transfers: usize,
        queue_capacity: usize,
    ) -> Result<(AuthorityHandle, JoinHandle<Result<(), DaemonError>>), DaemonError> {
        if queue_capacity == 0 || queue_capacity > 65_536 {
            return Err(DaemonError::new(
                DaemonErrorCode::InvalidInput,
                DaemonRecovery::CorrectRequest,
                "start authority owner",
                "authority queue capacity is outside production bounds",
            ));
        }
        let artifact_authority =
            ArtifactAuthority::new(artifacts, maximum_artifact_bytes, maximum_transfers)?;
        let (sender, receiver) = mpsc::channel(queue_capacity);
        let handle = AuthorityHandle::new(sender);
        let prompts = PromptBroker::new(PromptBrokerLimits::PRODUCTION);
        let task =
            tokio::spawn(runtime::run(journal, lifecycle, artifact_authority, prompts, receiver));
        Ok((handle, task))
    }
}
