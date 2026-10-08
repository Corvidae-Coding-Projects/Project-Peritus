//! Exact process-tree probing and deterministic restart classifications.

use peritus_types::{ActionId, ProcessId, RunId};

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
    /// A complete durable result exists and resource ownership is settled.
    Terminal,
    /// The exact live owned tree was found and remains live or termination was requested.
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

    /// Returns whether every record has a complete result and settled ownership.
    #[must_use]
    pub fn all_terminal(&self) -> bool {
        self.quarantined_records == 0
            && self.entries.iter().all(|entry| entry.disposition == RecoveryDisposition::Terminal)
    }
}

impl ProcessStore {
    /// Observes one exact durable owner without terminating or mutating it.
    ///
    /// Unlike [`Self::reconcile_exact`], this method is safe for ordinary receipt recovery: an
    /// exact live process is returned as `LiveOwned` and remains untouched. A missing manifest,
    /// mismatched claim, or unverifiable platform identity remains `Indeterminate`.
    ///
    /// # Errors
    ///
    /// Returns a typed recovery error when the native probe itself fails.
    pub fn observe_exact(
        &self,
        run_id: RunId,
        action_id: ActionId,
        process_id: ProcessId,
        probe: &mut impl ProcessProbe,
    ) -> Result<RecoveryEntry, ProcessError> {
        let (manifests, mut claims) = self.recovery_records();
        let Some(manifest) =
            manifests.into_iter().find(|manifest| manifest.identity.process_id() == process_id)
        else {
            return Ok(RecoveryEntry::new(process_id, RecoveryDisposition::Indeterminate, false));
        };
        let claim_matches =
            claims.remove(&process_id).is_some_and(|claim| claim.matches_manifest(&manifest));
        if !claim_matches
            || manifest.identity.run_id() != run_id
            || manifest.identity.action_id() != action_id
        {
            return Ok(RecoveryEntry::new(process_id, RecoveryDisposition::Indeterminate, false));
        }
        if manifest.phase == LifecyclePhase::Terminal && manifest.ownership_settled() {
            return Ok(RecoveryEntry::new(process_id, RecoveryDisposition::Terminal, false));
        }
        let Some(tree) = manifest.tree else {
            let disposition = if manifest.phase == LifecyclePhase::Authorized {
                RecoveryDisposition::AbsentUnobserved
            } else {
                RecoveryDisposition::Indeterminate
            };
            return Ok(RecoveryEntry::new(process_id, disposition, false));
        };
        let disposition = match probe.observe(tree)? {
            ProbeObservation::ExactLive => RecoveryDisposition::LiveOwned,
            ProbeObservation::ExactAbsent
                if manifest.phase == LifecyclePhase::Terminal && manifest.terminal.is_some() =>
            {
                // The terminal envelope exists, but incomplete cleanup/publication facts cannot
                // be repaired by a read-only receipt observer.
                RecoveryDisposition::Indeterminate
            }
            ProbeObservation::ExactAbsent => RecoveryDisposition::AbsentUnobserved,
            ProbeObservation::Mismatched | ProbeObservation::Unverifiable => {
                RecoveryDisposition::Indeterminate
            }
        };
        Ok(RecoveryEntry::new(process_id, disposition, false))
    }

    /// Reconciles one exact durable owner without probing or terminating unrelated processes.
    ///
    /// The run, action, and process identities must all match the retained manifest. A live exact
    /// tree is terminated and probed again; absence without a terminal result remains
    /// `AbsentUnobserved`, never success.
    ///
    /// # Errors
    ///
    /// Returns a typed recovery error when the native probe or durable ownership update fails.
    pub fn reconcile_exact(
        &self,
        run_id: RunId,
        action_id: ActionId,
        process_id: ProcessId,
        probe: &mut impl ProcessProbe,
    ) -> Result<RecoveryEntry, ProcessError> {
        let (manifests, mut claims) = self.recovery_records();
        let Some(manifest) =
            manifests.into_iter().find(|manifest| manifest.identity.process_id() == process_id)
        else {
            return Ok(RecoveryEntry::new(process_id, RecoveryDisposition::Indeterminate, false));
        };
        let claim_matches =
            claims.remove(&process_id).is_some_and(|claim| claim.matches_manifest(&manifest));
        if !claim_matches
            || manifest.identity.run_id() != run_id
            || manifest.identity.action_id() != action_id
        {
            return Ok(RecoveryEntry::new(process_id, RecoveryDisposition::Indeterminate, false));
        }
        if manifest.phase == LifecyclePhase::Terminal && manifest.ownership_settled() {
            return Ok(RecoveryEntry::new(process_id, RecoveryDisposition::Terminal, false));
        }
        let Some(tree) = manifest.tree else {
            if manifest.phase == LifecyclePhase::Authorized {
                self.reconcile_ownership(process_id, true)?;
                return Ok(RecoveryEntry::new(
                    process_id,
                    RecoveryDisposition::AbsentUnobserved,
                    false,
                ));
            }
            return Ok(RecoveryEntry::new(process_id, RecoveryDisposition::Indeterminate, false));
        };
        match probe.observe(tree)? {
            ProbeObservation::ExactAbsent => {
                self.reconcile_ownership(process_id, true)?;
                Ok(RecoveryEntry::new(
                    process_id,
                    if manifest.phase == LifecyclePhase::Terminal {
                        RecoveryDisposition::Terminal
                    } else {
                        RecoveryDisposition::AbsentUnobserved
                    },
                    false,
                ))
            }
            ProbeObservation::ExactLive => {
                probe.terminate(tree)?;
                self.reconcile_ownership(process_id, false)?;
                let disposition = match probe.observe(tree)? {
                    ProbeObservation::ExactAbsent => {
                        self.reconcile_ownership(process_id, true)?;
                        if manifest.phase == LifecyclePhase::Terminal {
                            RecoveryDisposition::Terminal
                        } else {
                            RecoveryDisposition::AbsentUnobserved
                        }
                    }
                    ProbeObservation::ExactLive => RecoveryDisposition::LiveOwned,
                    ProbeObservation::Mismatched | ProbeObservation::Unverifiable => {
                        RecoveryDisposition::Indeterminate
                    }
                };
                Ok(RecoveryEntry::new(process_id, disposition, true))
            }
            ProbeObservation::Mismatched | ProbeObservation::Unverifiable => {
                self.reconcile_ownership(process_id, false)?;
                Ok(RecoveryEntry::new(process_id, RecoveryDisposition::Indeterminate, false))
            }
        }
    }

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
        self.reconcile_preserving_exact_live(&[], probe)
    }

    /// Reconciles the process registry while preserving receipt-linked exact live owners.
    ///
    /// The preservation list contains exact `(run, action, process)` identities recovered from
    /// durable command receipts. Each identity is still checked against its manifest, claim, and
    /// native process-tree birth identity before it can be preserved. Unlisted, mismatched, or
    /// unverifiable owners retain the ordinary [`Self::reconcile`] behavior.
    ///
    /// # Errors
    ///
    /// Returns a typed error if probing, exact termination, or durable reconciliation fails.
    pub fn reconcile_preserving_exact_live(
        &self,
        preserved_owners: &[(RunId, ActionId, ProcessId)],
        probe: &mut impl ProcessProbe,
    ) -> Result<RecoveryReport, ProcessError> {
        let mut entries = Vec::new();
        let (manifests, mut claims) = self.recovery_records();
        for manifest in manifests {
            let process_id = manifest.identity.process_id();
            let exact_owner =
                (manifest.identity.run_id(), manifest.identity.action_id(), process_id);
            let claim_matches =
                claims.remove(&process_id).is_some_and(|claim| claim.matches_manifest(&manifest));
            let (disposition, signal_sent) = if !claim_matches {
                (RecoveryDisposition::Indeterminate, false)
            } else if manifest.phase == LifecyclePhase::Terminal && manifest.ownership_settled() {
                (RecoveryDisposition::Terminal, false)
            } else if let Some(tree) = manifest.tree {
                match probe.observe(tree)? {
                    ProbeObservation::ExactLive if preserved_owners.contains(&exact_owner) => {
                        (RecoveryDisposition::LiveOwned, false)
                    }
                    ProbeObservation::ExactLive => {
                        probe.terminate(tree)?;
                        self.reconcile_ownership(process_id, false)?;
                        (RecoveryDisposition::LiveOwned, true)
                    }
                    ProbeObservation::ExactAbsent => {
                        self.reconcile_ownership(process_id, true)?;
                        if manifest.phase == LifecyclePhase::Terminal {
                            (RecoveryDisposition::Terminal, false)
                        } else {
                            (RecoveryDisposition::AbsentUnobserved, false)
                        }
                    }
                    ProbeObservation::Mismatched | ProbeObservation::Unverifiable => {
                        self.reconcile_ownership(process_id, false)?;
                        (RecoveryDisposition::Indeterminate, false)
                    }
                }
            } else {
                self.reconcile_ownership(process_id, true)?;
                if manifest.phase == LifecyclePhase::Terminal {
                    (RecoveryDisposition::Terminal, false)
                } else {
                    (RecoveryDisposition::AbsentUnobserved, false)
                }
            };
            entries.push(RecoveryEntry::new(process_id, disposition, signal_sent));
        }
        entries.extend(claims.into_keys().map(|process_id| {
            RecoveryEntry::new(process_id, RecoveryDisposition::Indeterminate, false)
        }));
        Ok(RecoveryReport { entries, quarantined_records: self.quarantined_records().len() })
    }
}
