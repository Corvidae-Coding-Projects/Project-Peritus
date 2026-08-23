//! Safe names used to derive managed worktree paths.

use crate::{ErrorKind, GitError, Operation, RecoveryClass};

const MAX_WORKTREE_NAME_BYTES: usize = 96;

/// Validated portable worktree name.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct WorktreeName(String);

impl WorktreeName {
    /// Validates one non-option-like portable worktree name.
    ///
    /// # Errors
    ///
    /// Rejects empty, oversized, leading-dash, reserved, or nonportable names.
    pub fn new(value: impl Into<String>) -> Result<Self, GitError> {
        let value = value.into();
        let bytes = value.as_bytes();
        let valid = !bytes.is_empty()
            && bytes.len() <= MAX_WORKTREE_NAME_BYTES
            && bytes[0] != b'-'
            && bytes.iter().copied().all(crate::verified::worktree_name_byte_allowed)
            && !value.eq_ignore_ascii_case("git")
            && !value.eq_ignore_ascii_case("peritus");
        if !valid {
            return Err(GitError::new(
                ErrorKind::InvalidInput,
                Operation::CreateWorktree,
                RecoveryClass::CorrectRequest,
                "worktree name is not portable or is reserved",
            ));
        }
        Ok(Self(value))
    }

    /// Returns the validated name.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::WorktreeName;

    #[test]
    fn validates_portable_names() {
        assert_eq!(WorktreeName::new("run_42").expect("name").as_str(), "run_42");
        for invalid in ["", "-option", "has.dot", "has/slash", "git", "peritus"] {
            assert!(WorktreeName::new(invalid).is_err(), "accepted {invalid:?}");
        }
    }
}
