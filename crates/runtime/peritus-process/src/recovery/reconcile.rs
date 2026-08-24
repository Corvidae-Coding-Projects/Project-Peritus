//! Exact process-tree probing and deterministic restart classifications.

use peritus_types::ProcessId;

use crate::{LifecyclePhase, ProcessError, ProcessStore, ProcessTreeIdentity};

/// Exact observation made by a platform process-identity probe.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ProbeObservation {
    /// The durable birth identity still names the same live owned tree.
    ExactLive,
    /// No process exists for the durable root identity.
    ExactAbsent,
    /// The numeric root identity now names a different process.
    Mismatched,
    /// The platform cannot establish an exact identity relation.
    Unverifiable,
}

/// Injected platform boundary used during durable reconciliation.
pub trait ProcessProbe {
    /// Observes the current relation to one durable process-tree identity.
    ///
    /// # Errors
    ///
    /// Returns a typed recovery error when the platform observation itself fails.
    fn observe(&mut self, identity: ProcessTreeIdentity) -> Result<ProbeObservation, ProcessError>;

    /// Terminates only the exact live tree supplied by a preceding observation.
    ///
    /// # Errors
    ///
    /// Returns a typed process-tree error when termination cannot be completed.
    fn terminate(&mut self, identity: ProcessTreeIdentity) -> Result<(), ProcessError>;
}

/// Stable recovery outcome for one durable execution.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum RecoveryDisposition {
    /// The durable record was already terminal and required no platform action.
    AlreadyTerminal,
    /// The exact live owned tree was found and termination was requested.
    LiveOwned,
    /// The process was absent without a committed terminal observation.
    AbsentUnobserved,
    /// Identity or termination could not be established safely.
    Indeterminate,
}

/// Restart outcome for one process identity.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct RecoveryEntry {
    process_id: ProcessId,
    disposition: RecoveryDisposition,
    signal_sent: bool,
}

impl RecoveryEntry {
    const fn new(
        process_id: ProcessId,
        disposition: RecoveryDisposition,
        signal_sent: bool,
    ) -> Self {
        Self { process_id, disposition, signal_sent }
    }

    /// Returns the durable process identity.
    #[must_use]
    pub const fn process_id(self) -> ProcessId {
        self.process_id
    }

    /// Returns the deterministic restart classification.
    #[must_use]
    pub const fn disposition(self) -> RecoveryDisposition {
        self.disposition
    }

    /// Returns whether recovery requested termination of an exact live tree.
    #[must_use]
    pub const fn signal_sent(self) -> bool {
        self.signal_sent
    }
}

/// Complete bounded registry reconciliation report.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecoveryReport {
    entries: Vec<RecoveryEntry>,
    quarantined_records: usize,
}

impl RecoveryReport {
    /// Returns one outcome for every decoded durable manifest or orphan consumption claim.
    #[must_use]
    pub fn entries(&self) -> &[RecoveryEntry] {
        &self.entries
    }

    /// Returns the number of corrupt records quarantined while opening the registry.
    #[must_use]
    pub const fn quarantined_records(&self) -> usize {
        self.quarantined_records
    }

    /// Returns whether every record was already terminal.
    #[must_use]
    pub fn all_terminal(&self) -> bool {
        self.entries.iter().all(|entry| entry.disposition == RecoveryDisposition::AlreadyTerminal)
    }
}

impl ProcessStore {
    /// Reconciles every durable manifest using exact injected process observations.
    ///
    /// Only [`ProbeObservation::ExactLive`] permits a termination request. Absence is never
    /// converted into successful completion, and mismatched or unverifiable identities remain
    /// indeterminate.
    ///
    /// # Errors
    ///
    /// Returns a typed error if probing, exact termination, or durable reconciliation fails.
    pub fn reconcile(&self, probe: &mut impl ProcessProbe) -> Result<RecoveryReport, ProcessError> {
        let mut entries = Vec::new();
        let (manifests, mut claims) = self.recovery_records();
        for manifest in manifests {
            let process_id = manifest.identity.process_id();
            let claim_matches =
                claims.remove(&process_id).is_some_and(|claim| claim.matches_manifest(&manifest));
            let (disposition, signal_sent) = if !claim_matches {
                (RecoveryDisposition::Indeterminate, false)
            } else if manifest.phase == LifecyclePhase::Terminal {
                (RecoveryDisposition::AlreadyTerminal, false)
            } else if let Some(tree) = manifest.tree {
                match probe.observe(tree)? {
                    ProbeObservation::ExactLive => {
                        probe.terminate(tree)?;
                        self.reconcile_manifest(process_id, false)?;
                        (RecoveryDisposition::LiveOwned, true)
                    }
                    ProbeObservation::ExactAbsent => {
                        self.reconcile_manifest(process_id, true)?;
                        (RecoveryDisposition::AbsentUnobserved, false)
                    }
                    ProbeObservation::Mismatched | ProbeObservation::Unverifiable => {
                        self.reconcile_manifest(process_id, false)?;
                        (RecoveryDisposition::Indeterminate, false)
                    }
                }
            } else {
                self.reconcile_manifest(process_id, true)?;
                (RecoveryDisposition::AbsentUnobserved, false)
            };
            entries.push(RecoveryEntry::new(process_id, disposition, signal_sent));
        }
        entries.extend(claims.into_keys().map(|process_id| {
            RecoveryEntry::new(process_id, RecoveryDisposition::Indeterminate, false)
        }));
        Ok(RecoveryReport { entries, quarantined_records: self.quarantined_records().len() })
    }
}
