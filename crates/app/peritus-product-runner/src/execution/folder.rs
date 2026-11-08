//! In-place workspace capability supplied to the shared conversational/pipeline entry point.

use super::{ProductRunInput, ProductRunOutcome, ProductRunner, RunObserver};
use crate::{ConversationMode, ProductRunnerError, ProductRunnerErrorKind, ProductWorkspaceKind};
use std::path::PathBuf;

impl ProductRunner {
    /// Runs an already-authorized production continuation in an ordinary directory.
    ///
    /// Folder delivery is an execution capability, not a conversation-mode side effect. Exact
    /// retries use this entry point so resume validation observes the same in-place scope that
    /// produced the retained candidate.
    ///
    /// # Errors
    /// Returns an invalid precondition for a read-only folder, or propagates pipeline failures.
    pub async fn run_folder(
        mut input: ProductRunInput,
        writable: bool,
        protected: &[PathBuf],
        observe: RunObserver,
    ) -> Result<ProductRunOutcome, ProductRunnerError> {
        if !writable {
            return Err(ProductRunnerError::new(
                ProductRunnerErrorKind::InvalidPrecondition,
                "resume in-place product run",
                "the selected folder is read-only; enable write access before retrying",
            ));
        }
        bind(&mut input, protected);
        Box::pin(Self::run(input, observe)).await
    }

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
        bind(&mut input, protected);
        Box::pin(Self::converse_scoped(input, mode, writable, observe)).await
    }
}

fn bind(input: &mut ProductRunInput, protected: &[PathBuf]) {
    let baseline_revision =
        input.resume.as_ref().and_then(|resume| resume.baseline().scope()).map_or_else(
            || input.conversation.revision(),
            crate::workspace_delivery::scope::ScopedBaseline::revision,
        );
    input.workspace_kind =
        ProductWorkspaceKind::InPlace { protected_paths: protected.to_vec(), baseline_revision };
}
