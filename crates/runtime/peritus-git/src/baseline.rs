//! One-time resolution of immutable commit/tree baselines.

use std::ffi::OsString;

use crate::command::{CommandAccess, one_line};
use crate::repository::strings;
use crate::{
    CommitId, ErrorKind, GitError, GitRepository, ObjectId, Operation, RecoveryClass, TreeId,
};

const MAX_REVISION_BYTES: usize = 1_024;

/// Immutable commit and root tree selected once from a revision expression.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct Baseline {
    commit: CommitId,
    tree: TreeId,
}

impl Baseline {
    pub(crate) const fn checked(commit: CommitId, tree: TreeId) -> Self {
        Self { commit, tree }
    }

    /// Returns the exact peeled commit identity.
    #[must_use]
    pub const fn commit(self) -> CommitId {
        self.commit
    }

    /// Returns the exact root tree identity.
    #[must_use]
    pub const fn tree(self) -> TreeId {
        self.tree
    }
}

impl GitRepository {
    /// Resolves one revision expression exactly once to an immutable commit and tree.
    ///
    /// # Errors
    ///
    /// Rejects option-like or control-bearing expressions, missing commits, and malformed IDs.
    pub fn resolve_baseline(&self, revision: &str) -> Result<Baseline, GitError> {
        validate_revision(revision)?;
        let commit_expression = format!("{revision}^{{commit}}");
        let mut arguments = strings(&["rev-parse", "--verify", "--end-of-options"]);
        arguments.push(OsString::from(commit_expression));
        let output = self.checked_repo_command(
            Operation::ResolveBaseline,
            CommandAccess::Read,
            &arguments,
            None,
        )?;
        let commit = CommitId::checked(ObjectId::parse(
            self.identity.object_format(),
            one_line(&output.stdout, Operation::ResolveBaseline)?,
            Operation::ResolveBaseline,
        )?);
        let tree_expression = format!("{}^{{tree}}", commit.object_id());
        let mut arguments = strings(&["rev-parse", "--verify", "--end-of-options"]);
        arguments.push(OsString::from(tree_expression));
        let output = self.checked_repo_command(
            Operation::ResolveBaseline,
            CommandAccess::Read,
            &arguments,
            None,
        )?;
        let tree = TreeId::checked(ObjectId::parse(
            self.identity.object_format(),
            one_line(&output.stdout, Operation::ResolveBaseline)?,
            Operation::ResolveBaseline,
        )?);
        Ok(Baseline::checked(commit, tree))
    }
}

fn validate_revision(revision: &str) -> Result<(), GitError> {
    let valid = !revision.is_empty()
        && revision.len() <= MAX_REVISION_BYTES
        && !revision.starts_with('-')
        && !revision.bytes().any(|byte| byte == 0 || byte.is_ascii_control());
    if valid {
        Ok(())
    } else {
        Err(GitError::new(
            ErrorKind::InvalidInput,
            Operation::ResolveBaseline,
            RecoveryClass::CorrectRequest,
            "revision expression is empty, oversized, option-like, or contains controls",
        ))
    }
}
