//! In-place workspace capability supplied to the shared conversational/pipeline entry point.

use super::{ProductRunInput, ProductRunOutcome, ProductRunner, RunObserver};
use crate::{ConversationMode, ProductRunnerError, ProductWorkspaceKind};
use std::path::PathBuf;

impl ProductRunner {
    /// Runs a conversation in an ordinary directory, with authorized implementation delegated to
    /// the same production pipeline. Effects remain in place; no Git baseline or rollback is made.
    ///
    /// # Errors
    /// Returns input, persistence or execution failures without scanning unrelated folder content.
    pub async fn converse_folder(
        mut input: ProductRunInput,
        mode: ConversationMode,
        writable: bool,
        protected: &[PathBuf],
        observe: RunObserver,
    ) -> Result<ProductRunOutcome, ProductRunnerError> {
        let baseline_revision =
            input.resume.as_ref().and_then(|resume| resume.baseline().scope()).map_or_else(
                || input.conversation.revision(),
                crate::workspace_delivery::scope::ScopedBaseline::revision,
            );
        input.workspace_kind = ProductWorkspaceKind::InPlace {
            protected_paths: protected.to_vec(),
            baseline_revision,
        };
        Box::pin(Self::converse_scoped(input, mode, writable, observe)).await
    }
}
