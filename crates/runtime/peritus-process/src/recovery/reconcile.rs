//! Exact process-tree probing and deterministic restart classifications.

use peritus_types::ProcessId;

use crate::{
    LifecyclePhase, ProcessCursor, ProcessError, ProcessStore, ProcessTreeIdentity,
    RetainedOwnerRequest, RetainedProcessKey,
};

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

/// Observation of the complete durable containment, independent of its root process.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ProcessTreeQuiescence {
    /// The complete owned containment has no remaining processes.
    Quiescent,
    /// Complete containment absence has not been established.
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

    /// Observes whether every process in the durable containment is absent.
    ///
    /// Root absence alone cannot establish this fact: descendants may outlive their parent.
    /// Probes that cannot inspect the complete containment retain unresolved ownership.
    ///
    /// # Errors
    /// Returns the original native failure when the containment observation fails.
    fn observe_quiescence(
        &mut self,
        _identity: ProcessTreeIdentity,
    ) -> Result<ProcessTreeQuiescence, ProcessError> {
        Ok(ProcessTreeQuiescence::Unverifiable)
    }

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
    /// The exact process remains owned: either retained control was reattached or legacy
    /// recovery requested termination of the exact live tree.
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

/// One recovery classification with the exact scoped probe failure, when available.
///
/// A failed observation never grants termination or settlement authority. Its durable record
/// remains available for a later reconciliation while other records continue to be observed.
#[derive(Debug)]
pub struct RecoveryObservation {
    entry: RecoveryEntry,
    failure: Option<ProcessError>,
}

impl RecoveryObservation {
    const fn completed(entry: RecoveryEntry) -> Self {
        Self { entry, failure: None }
    }

    const fn failed(process_id: ProcessId, failure: ProcessError) -> Self {
        Self {
            entry: RecoveryEntry::new(process_id, RecoveryDisposition::Indeterminate, false),
            failure: Some(failure),
        }
    }

    /// Returns the exact identity and deterministic classification.
    #[must_use]
    pub const fn entry(&self) -> RecoveryEntry {
        self.entry
    }

    /// Returns the original platform failure without discarding its typed cause.
    #[must_use]
    pub const fn failure(&self) -> Option<&ProcessError> {
        self.failure.as_ref()
    }

    /// Transfers both classification and diagnostic to the recovery owner.
    #[must_use]
    pub fn into_parts(self) -> (RecoveryEntry, Option<ProcessError>) {
        (self.entry, self.failure)
    }
}

/// Complete registry reconciliation report collected by the compatibility API.
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
    /// Reconciles every durable manifest using exact injected process observations.
    ///
    /// Only [`ProbeObservation::ExactLive`] permits a termination request. Absence is never
    /// converted into successful completion, and mismatched or unverifiable identities remain
    /// indeterminate.
    ///
    /// # Errors
    ///
    /// Returns a typed storage, durable reconciliation, or result-consumer error. A scoped probe
    /// or termination failure is reported as an indeterminate record and does not abort the scan.
    pub fn reconcile(&self, probe: &mut impl ProcessProbe) -> Result<RecoveryReport, ProcessError> {
        let mut entries = Vec::new();
        let quarantined_records = self.reconcile_with(probe, |entry| {
            entries.push(entry);
            Ok(())
        })?;
        Ok(RecoveryReport { entries, quarantined_records })
    }

    /// Reconciles durable records in bounded pages and delivers each exact outcome to its owner.
    ///
    /// This avoids retaining the lifetime registry or its report in memory. Every accepted identity
    /// remains visible, including immutable terminal tombstones and orphan consumption claims.
    ///
    /// # Errors
    /// Returns a typed storage, durable reconciliation, or result-consumer error. Scoped native
    /// failures are indeterminate outcomes; use [`Self::reconcile_observations`] to retain their
    /// original typed diagnostics rather than the compatibility classification alone.
    pub fn reconcile_with(
        &self,
        probe: &mut impl ProcessProbe,
        mut observe: impl FnMut(RecoveryEntry) -> Result<(), ProcessError>,
    ) -> Result<usize, ProcessError> {
        self.reconcile_observations(probe, |observation| observe(observation.entry()))
    }

    /// Reconciles every record while delivering scoped native failures with their exact identity.
    ///
    /// Probe and termination failures leave the predecessor manifest unchanged. Every other
    /// trusted record remains eligible for reconciliation in the same scan. Callers may retain,
    /// display, or retry each move-only diagnostic without collecting a lifetime registry.
    ///
    /// # Errors
    /// Returns a typed error when the registry frontier, durable reconciliation, or consumer
    /// fails; these failures cannot safely be represented as an absent or settled native tree.
    pub fn reconcile_observations(
        &self,
        probe: &mut impl ProcessProbe,
        mut observe: impl FnMut(RecoveryObservation) -> Result<(), ProcessError>,
    ) -> Result<usize, ProcessError> {
        let mut after = None;
        loop {
            let page = self.registry_page(after)?;
            if page.is_empty() { break; }
            for record in page {
                let process_id = record.process_id;
                after = Some(process_id);
                let Some(record) = self.authoritative_record(process_id)? else {
                    continue;
                };
                let Some(manifest) = record.manifest else {
                    observe(RecoveryObservation::completed(RecoveryEntry::new(
                        process_id, RecoveryDisposition::Indeterminate, false,
                    )))?;
                    continue;
                };
                let claim_matches = record.claim.is_some_and(|claim| claim.matches_manifest(&manifest));
                let retained = record.claim.and_then(|claim| claim.retained_owner());
                let retained_disposition = if claim_matches
                    && retained.is_some()
                    && !(manifest.phase == LifecyclePhase::Terminal
                        && manifest.ownership_settled())
                {
                    match self.reconnect_retained_owner(process_id) {
                        Ok(disposition) => disposition,
                        Err(error) => {
                            observe(RecoveryObservation::failed(process_id, error))?;
                            continue;
                        }
                    }
                } else {
                    None
                };
                let (disposition, signal_sent) = if let Some(disposition) = retained_disposition {
                    (disposition, false)
                } else if !claim_matches {
                    (RecoveryDisposition::Indeterminate, false)
                } else if manifest.phase == LifecyclePhase::Terminal && manifest.ownership_settled() {
                    (RecoveryDisposition::Terminal, false)
                } else if let Some(tree) = manifest.tree {
                    let native = match probe.observe(tree) {
                        Ok(native) => native,
                        Err(error) => {
                            observe(RecoveryObservation::failed(process_id, error))?;
                            continue;
                        }
                    };
                    match native {
                        ProbeObservation::ExactLive => {
                            if let Err(error) = probe.terminate(tree) {
                                observe(RecoveryObservation::failed(process_id, error))?;
                                continue;
                            }
                            self.reconcile_ownership(&manifest, false)?;
                            (RecoveryDisposition::LiveOwned, true)
                        }
                        ProbeObservation::ExactAbsent => {
                            let quiescence = match probe.observe_quiescence(tree) {
                                Ok(quiescence) => quiescence,
                                Err(error) => {
                                    observe(RecoveryObservation::failed(process_id, error))?;
                                    continue;
                                }
                            };
                            let tree_quiescent = quiescence == ProcessTreeQuiescence::Quiescent;
                            let applied = self.reconcile_ownership(&manifest, tree_quiescent)?;
                            if !applied || !tree_quiescent {
                                (RecoveryDisposition::Indeterminate, false)
                            } else if manifest.phase == LifecyclePhase::Terminal {
                                (RecoveryDisposition::Terminal, false)
                            } else {
                                (RecoveryDisposition::AbsentUnobserved, false)
                            }
                        }
                        ProbeObservation::Mismatched | ProbeObservation::Unverifiable => {
                            self.reconcile_ownership(&manifest, false)?;
                            (RecoveryDisposition::Indeterminate, false)
                        }
                    }
                } else {
                    // Starting may have crossed the native effect boundary before recording its
                    // tree. A missing durable identity cannot prove that such an effect is absent.
                    let tree_quiescent = manifest.tree_quiescent
                        || manifest.phase == LifecyclePhase::Authorized;
                    let applied = self.reconcile_ownership(&manifest, tree_quiescent)?;
                    if !applied || !tree_quiescent {
                        (RecoveryDisposition::Indeterminate, false)
                    } else if manifest.phase == LifecyclePhase::Terminal {
                        (RecoveryDisposition::Terminal, false)
                    } else {
                        (RecoveryDisposition::AbsentUnobserved, false)
                    }
                };
                observe(RecoveryObservation::completed(RecoveryEntry::new(
                    process_id, disposition, signal_sent,
                )))?;
            }
        }
        Ok(self.quarantined_records().len())
    }

    fn reconnect_retained_owner(
        &self,
        process_id: ProcessId,
    ) -> Result<Option<RecoveryDisposition>, ProcessError> {
        let Some(transport) = self.retained_owner_transport() else {
            // A direct daemon owns no service-lifetime process transport and retains the legacy
            // exact-probe cleanup behavior below.
            return Ok(None);
        };
        let reservation = self
            .retained_owner_reservation(process_id)?
            .ok_or_else(|| retained_recovery_error("retained owner request is missing"))?;
        let mut request_bytes = Vec::new();
        request_bytes
            .try_reserve_exact(reservation.request().len())
            .map_err(|_| retained_recovery_error("retained owner request cannot be allocated"))?;
        request_bytes.extend_from_slice(reservation.request());
        let request = RetainedOwnerRequest::decode(request_bytes)?;
        let binding = request.binding();
        let key = RetainedProcessKey::from_binding(binding);
        if binding.process_id() != process_id
            || binding.operation_digest() != reservation.operation_digest()
            || request.digest() != reservation.request_digest()
            || binding.service_owner() != transport.service_owner()
        {
            return Err(retained_recovery_error(
                "retained owner request differs from the active service generation",
            ));
        }
        transport.launch_or_attach(key, request.encode(), request.digest())?;
        let observation = transport.observe(key, ProcessCursor::after(0), 0, None)?;
        if request.backend_factory_request().platform() == crate::NativePlatform::Macos
            && !observation.matches_native_adoption(
                request.backend_factory_request().platform(),
                binding,
            )
        {
            return Err(retained_recovery_error(
                "retained native session custody is unavailable or differs",
            ));
        }
        self.refresh_authoritative_identity(process_id)?;
        if observation.owner_finished()
            && observation.terminal_result().is_some()
            && self.terminal_result(process_id).is_ok()
        {
            return Ok(Some(RecoveryDisposition::Terminal));
        }
        Ok(Some(RecoveryDisposition::LiveOwned))
    }
}

const fn retained_recovery_error(detail: &'static str) -> ProcessError {
    ProcessError::new(
        crate::ErrorCode::Indeterminate,
        crate::ProcessOperation::Reconcile,
        crate::RecoveryClass::ReopenAndReconcile,
        detail,
    )
}
