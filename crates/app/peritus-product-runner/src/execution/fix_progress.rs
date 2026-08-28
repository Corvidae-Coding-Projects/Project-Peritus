//! Cross-cycle candidate and review progress for the E0 fixer loop.

use std::{collections::BTreeMap, path::Path};

use peritus_review::ProductFindingLedger;
use peritus_types::Sha256Digest;

use crate::{ProductRunnerError, progress::WorkspaceCheckpoint};

const MAX_CONSECUTIVE_UNCHANGED_FIXES: u8 = 2;
const MAX_CONSECUTIVE_UNRESOLVED_FIXES: u8 = 2;

pub(super) struct FixProgress {
    checkpoint: WorkspaceCheckpoint,
    consecutive_unchanged: u8,
    blocking_attempts: BTreeMap<Sha256Digest, u8>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum FixProgressObservation {
    Changed,
    Unchanged,
    Exhausted,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct PersistentFinding {
    pub(super) title: String,
    pub(super) location: String,
}

impl FixProgress {
    pub(super) fn capture(root: &Path) -> Result<Self, ProductRunnerError> {
        Ok(Self {
            checkpoint: WorkspaceCheckpoint::capture(root)?,
            consecutive_unchanged: 0,
            blocking_attempts: BTreeMap::new(),
        })
    }

    pub(super) fn reset(&mut self, root: &Path) -> Result<(), ProductRunnerError> {
        self.checkpoint = WorkspaceCheckpoint::capture(root)?;
        self.consecutive_unchanged = 0;
        self.blocking_attempts.clear();
        Ok(())
    }

    /// Observes one fresh review and returns a blocker that survived the configured number of
    /// complete fixer/reviewer attempts. Candidate-byte changes deliberately do not reset this
    /// signal: a fixer is making useful progress only when the blocking defect itself changes or
    /// disappears.
    pub(super) fn observe_findings(
        &mut self,
        findings: &ProductFindingLedger,
    ) -> Option<PersistentFinding> {
        let mut current = BTreeMap::new();
        let mut exhausted = None;
        for finding in findings.open_findings().filter(|finding| finding.blocking()) {
            let attempts = self
                .blocking_attempts
                .get(&finding.id())
                .map_or(0, |attempts| attempts.saturating_add(1));
            current.insert(finding.id(), attempts);
            if attempts >= MAX_CONSECUTIVE_UNRESOLVED_FIXES && exhausted.is_none() {
                exhausted = Some(PersistentFinding {
                    title: finding.title().to_owned(),
                    location: finding.location().to_owned(),
                });
            }
        }
        self.blocking_attempts = current;
        exhausted
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
    use peritus_review::{FindingSeverity, ProductFinding, ProductFindingCategory};

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

    #[test]
    fn changing_candidate_does_not_hide_a_persistent_blocker() {
        let root = repository();
        let mut progress = FixProgress::capture(root.path()).expect("initial progress");
        let ledger = review_ledger(1, "Canonical reason contradicts the case");
        assert_eq!(progress.observe_findings(&ledger), None);
        fs::write(root.path().join("candidate.txt"), "first fix").expect("first candidate");
        assert_eq!(
            progress.observe(root.path()).expect("first changed candidate"),
            FixProgressObservation::Changed
        );
        let ledger = review_ledger(2, "Canonical reason contradicts the case");
        assert_eq!(progress.observe_findings(&ledger), None);
        fs::write(root.path().join("candidate.txt"), "second fix").expect("second candidate");
        assert_eq!(
            progress.observe(root.path()).expect("second changed candidate"),
            FixProgressObservation::Changed
        );
        let ledger = review_ledger(3, "Canonical reason contradicts the case");

        assert_eq!(
            progress.observe_findings(&ledger),
            Some(PersistentFinding {
                title: "Canonical reason contradicts the case".to_owned(),
                location: "out/case_rulings.json".to_owned(),
            })
        );
    }

    #[test]
    fn a_changed_blocker_identity_starts_a_fresh_attempt_budget() {
        let root = repository();
        let mut progress = FixProgress::capture(root.path()).expect("initial progress");
        let ledger = review_ledger(1, "First blocker");
        assert_eq!(progress.observe_findings(&ledger), None);
        let ledger = review_ledger(2, "Second blocker");

        assert_eq!(progress.observe_findings(&ledger), None);
    }

    fn review_ledger(cycle: u32, title: &str) -> ProductFindingLedger {
        let finding = ProductFinding::new(
            ProductFindingCategory::RequestedBehavior,
            FindingSeverity::High,
            title.to_owned(),
            "The required canonical value and case evidence cannot both be represented.".to_owned(),
            "out/case_rulings.json".to_owned(),
            "Inspect the policy and generated ruling.".to_owned(),
            "Reconcile the explicit contradiction.".to_owned(),
            cycle,
        )
        .expect("finding");
        ProductFindingLedger::restore(cycle, "fresh review".to_owned(), vec![finding])
            .expect("review ledger")
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
