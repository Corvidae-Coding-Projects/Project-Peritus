//! Cross-cycle progress tracking for the E0 fixer loop.

use std::path::Path;

use crate::{ProductRunnerError, progress::WorkspaceCheckpoint};

const MAX_CONSECUTIVE_UNCHANGED_FIXES: u8 = 2;

pub(super) struct FixProgress {
    checkpoint: WorkspaceCheckpoint,
    consecutive_unchanged: u8,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum FixProgressObservation {
    Changed,
    Unchanged,
    Exhausted,
}

impl FixProgress {
    pub(super) fn capture(root: &Path) -> Result<Self, ProductRunnerError> {
        Ok(Self { checkpoint: WorkspaceCheckpoint::capture(root)?, consecutive_unchanged: 0 })
    }

    pub(super) fn reset(&mut self, root: &Path) -> Result<(), ProductRunnerError> {
        self.checkpoint = WorkspaceCheckpoint::capture(root)?;
        self.consecutive_unchanged = 0;
        Ok(())
    }

    pub(super) fn observe(
        &mut self,
        root: &Path,
    ) -> Result<FixProgressObservation, ProductRunnerError> {
        let current = WorkspaceCheckpoint::capture(root)?;
        if current != self.checkpoint {
            self.checkpoint = current;
            self.consecutive_unchanged = 0;
            return Ok(FixProgressObservation::Changed);
        }

        self.consecutive_unchanged = self.consecutive_unchanged.saturating_add(1);
        if self.consecutive_unchanged >= MAX_CONSECUTIVE_UNCHANGED_FIXES {
            Ok(FixProgressObservation::Exhausted)
        } else {
            Ok(FixProgressObservation::Unchanged)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, process::Command};

    use super::*;

    #[test]
    fn two_unchanged_fixer_cycles_exhaust_progress() {
        let root = repository();
        let mut progress = FixProgress::capture(root.path()).expect("initial progress");

        assert_eq!(
            progress.observe(root.path()).expect("first observation"),
            FixProgressObservation::Unchanged
        );
        assert_eq!(
            progress.observe(root.path()).expect("second observation"),
            FixProgressObservation::Exhausted
        );
    }

    #[test]
    fn candidate_change_resets_the_unchanged_count() {
        let root = repository();
        let mut progress = FixProgress::capture(root.path()).expect("initial progress");
        assert_eq!(
            progress.observe(root.path()).expect("first observation"),
            FixProgressObservation::Unchanged
        );

        fs::write(root.path().join("candidate.txt"), "changed").expect("change candidate");
        assert_eq!(
            progress.observe(root.path()).expect("changed observation"),
            FixProgressObservation::Changed
        );
        assert_eq!(
            progress.observe(root.path()).expect("new first observation"),
            FixProgressObservation::Unchanged
        );
    }

    fn repository() -> tempfile::TempDir {
        let root = tempfile::tempdir().expect("root");
        run(root.path(), &["init", "--quiet"]);
        run(root.path(), &["config", "user.email", "peritus@example.invalid"]);
        run(root.path(), &["config", "user.name", "Peritus Test"]);
        fs::write(root.path().join("candidate.txt"), "baseline").expect("write baseline");
        run(root.path(), &["add", "."]);
        run(root.path(), &["commit", "--quiet", "-m", "fixture"]);
        root
    }

    fn run(root: &Path, arguments: &[&str]) {
        assert!(
            Command::new("git").args(arguments).current_dir(root).status().expect("git").success()
        );
    }
}
