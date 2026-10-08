//! Digest-valid phase resume planning over retained product-run state.

use std::path::PathBuf;

use peritus_review::{ProductFindingBodyPublisher, ProductFindingLedger};
use peritus_run_settlement::CandidateCheckpoint;

mod durable;
mod gate_state;
mod hashing;
mod knowledge;

use knowledge::RoleKnowledge;

use super::ProductRunPhase;
use crate::{
    ProductRunnerError, ProductRunnerErrorKind, candidate::CandidateBaseline,
    design::DesignDocument, developer_tools::SuccessfulCommand,
};

/// Opaque retained state sufficient to continue at the first stale or missing phase.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProductRunResume {
    checkpoint: CandidateCheckpoint,
    baseline: CandidateBaseline,
    next_phase: ProductRunPhase,
    design: Option<RetainedDesign>,
    task_summary: String,
    run_instructions: String,
    fix_summaries: Vec<String>,
    tool_calls: u64,
    findings: RetainedFindings,
    diff: String,
    gates: String,
    review: String,
    gate_report: Option<crate::gates::GateReport>,
    developer_evidence: String,
    successful_commands: Vec<SuccessfulCommand>,
    fixer_cycles: u32,
    obligation_source_root: Option<peritus_types::Sha256Digest>,
    knowledge: Option<RoleKnowledge>,
}

/// Complete retained execution values copied into a resume handoff.
#[cfg(test)]
pub(super) struct ResumeCapture {
    pub(super) checkpoint: CandidateCheckpoint,
    pub(super) baseline: CandidateBaseline,
    pub(super) next_phase: ProductRunPhase,
    pub(super) design_path: PathBuf,
    pub(super) design_markdown: String,
    pub(super) design_revision: u64,
    pub(super) task_summary: String,
    pub(super) run_instructions: String,
    pub(super) fix_summaries: Vec<String>,
    pub(super) tool_calls: u64,
    pub(super) finding_state: String,
    pub(super) diff: String,
    pub(super) gates: String,
    pub(super) review: String,
    pub(super) gate_report: Option<crate::gates::GateReport>,
    pub(super) developer_evidence: String,
    pub(super) successful_commands: Vec<SuccessfulCommand>,
    pub(super) fixer_cycles: u32,
    pub(super) transcript: String,
}

/// Complete authoritative values retained before the candidate reducer settles.
pub(super) struct RetainedResumeCapture {
    pub(super) checkpoint: CandidateCheckpoint,
    pub(super) baseline: CandidateBaseline,
    pub(super) next_phase: ProductRunPhase,
    pub(super) design: Option<RetainedDesign>,
    pub(super) task_summary: String,
    pub(super) run_instructions: String,
    pub(super) fix_summaries: Vec<String>,
    pub(super) tool_calls: u64,
    pub(super) findings: RetainedFindings,
    pub(super) diff: String,
    pub(super) gates: String,
    pub(super) review: String,
    pub(super) gate_report: Option<crate::gates::GateReport>,
    pub(super) developer_evidence: String,
    pub(super) successful_commands: Vec<SuccessfulCommand>,
    pub(super) fixer_cycles: u32,
    pub(super) transcript: String,
}

/// Exact completed design values, when a design was actually published.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct RetainedDesign {
    path: PathBuf,
    markdown: String,
    conversation_revision: u64,
}

/// Authoritative finding state independent of its fallible serialized projection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum RetainedFindings {
    Typed(ProductFindingLedger),
    Encoded(String),
}

impl ProductRunResume {
    #[cfg(test)]
    pub(super) fn capture(values: ResumeCapture) -> Result<Self, ProductRunnerError> {
        let design = RetainedDesign::new(
            values.design_path,
            values.design_markdown,
            values.design_revision,
        )?;
        let (resume, diagnostic) = Self::capture_retained(RetainedResumeCapture {
            checkpoint: values.checkpoint,
            baseline: values.baseline,
            next_phase: values.next_phase,
            design: Some(design),
            task_summary: values.task_summary,
            run_instructions: values.run_instructions,
            fix_summaries: values.fix_summaries,
            tool_calls: values.tool_calls,
            findings: RetainedFindings::from_encoded(values.finding_state),
            diff: values.diff,
            gates: values.gates,
            review: values.review,
            gate_report: values.gate_report,
            developer_evidence: values.developer_evidence,
            successful_commands: values.successful_commands,
            fixer_cycles: values.fixer_cycles,
            transcript: values.transcript,
        });
        match diagnostic {
            Some(error) => Err(error),
            None => Ok(resume),
        }
    }

    pub(super) fn capture_retained(
        values: RetainedResumeCapture,
    ) -> (Self, Option<ProductRunnerError>) {
        let mut diagnostics = Vec::new();
        let findings_projection = match values.findings.knowledge_projection() {
            Ok(projection) => Some(projection),
            Err(error) => {
                diagnostics.push(error);
                None
            }
        };
        let knowledge = match (values.design.as_ref(), findings_projection.as_ref()) {
            (Some(design), Some(findings)) => match RoleKnowledge::capture(
                *values.checkpoint.identity(),
                &values.transcript,
                design.markdown(),
                findings,
                &values.developer_evidence,
            ) {
                Ok(knowledge) => Some(knowledge),
                Err(error) => {
                    diagnostics.push(error);
                    None
                }
            },
            (None, _) => {
                diagnostics.push(missing_design());
                None
            }
            (Some(_), None) => None,
        };
        let gate_report_is_stale = values
            .gate_report
            .as_ref()
            .is_some_and(|report| !gate_state::report_is_current(report, &values.checkpoint));
        let gate_report = values
            .gate_report
            .filter(|report| gate_state::report_is_current(report, &values.checkpoint));
        let gate_diagnostic = gate_report_is_stale.then(|| {
            ProductRunnerError::new(
                ProductRunnerErrorKind::Gate,
                "capture candidate-bound gate state",
                "the supplied gate report did not bind the captured checkpoint and was discarded",
            )
        });
        diagnostics.extend(gate_diagnostic);
        let next_phase = if values.design.is_none() {
            ProductRunPhase::Designing
        } else {
            phase_with_retained_evidence(
                values.next_phase,
                &values.checkpoint,
                gate_report.as_ref(),
            )
        };
        let resume = Self {
            checkpoint: values.checkpoint,
            baseline: values.baseline,
            next_phase,
            design: values.design,
            task_summary: values.task_summary,
            run_instructions: values.run_instructions,
            fix_summaries: values.fix_summaries,
            tool_calls: values.tool_calls,
            findings: values.findings,
            diff: values.diff,
            gates: values.gates,
            review: values.review,
            gate_report,
            developer_evidence: values.developer_evidence,
            successful_commands: values.successful_commands,
            fixer_cycles: values.fixer_cycles,
            obligation_source_root: None,
            knowledge,
        };
        (resume, combined_capture_diagnostic(diagnostics))
    }

    /// Exact candidate checkpoint at the interruption boundary.
    #[must_use]
    pub const fn checkpoint(&self) -> &CandidateCheckpoint {
        &self.checkpoint
    }

    /// Whether this continuation retains a fully verified gate report for its exact checkpoint.
    ///
    /// Hosts use this after durable decode to distinguish reusable execution evidence from legacy,
    /// missing, opaque, or stale continuation state.
    #[must_use]
    pub fn retains_current_gate_state(&self) -> bool {
        self.gate_report
            .as_ref()
            .is_some_and(|report| gate_state::report_is_current(report, &self.checkpoint))
    }

    pub(super) const fn baseline(&self) -> &CandidateBaseline {
        &self.baseline
    }

    /// First phase that was stale or incomplete when the run stopped.
    #[must_use]
    pub const fn next_phase(&self) -> ProductRunPhase {
        self.next_phase
    }

    /// Encodes this opaque continuation as a versioned durable payload.
    ///
    /// # Errors
    ///
    /// Returns an internal serialization error if the continuation cannot be encoded.
    pub fn encode_durable(&self) -> Result<Vec<u8>, ProductRunnerError> {
        durable::encode(self)
    }

    /// Streams this continuation into a caller-owned durable sink.
    ///
    /// The sink may split the serialization into physical pages without first allocating a second
    /// copy of the complete retained state.
    ///
    /// # Errors
    ///
    /// Returns an internal serialization error, including an error reported by the sink.
    pub fn encode_durable_into(
        &self,
        writer: impl std::io::Write,
    ) -> Result<(), ProductRunnerError> {
        durable::encode_into(self, writer)
    }

    /// Restores an opaque continuation from a versioned durable payload and current conversation.
    ///
    /// # Errors
    ///
    /// Rejects malformed, unsupported, or internally inconsistent retained state.
    pub fn decode_durable(bytes: &[u8], transcript: &str) -> Result<Self, ProductRunnerError> {
        durable::decode(bytes, transcript)
    }

    /// Restores the exact stored continuation before the host reconciles current candidate facts.
    ///
    /// Hosts use this while loading a record so crash-safe handoff recovery can inspect the
    /// original checkpoint before execution-context invalidation occurs.
    ///
    /// # Errors
    ///
    /// Rejects malformed, unsupported, or internally inconsistent retained state.
    pub fn decode_durable_retained(
        bytes: &[u8],
        transcript: &str,
    ) -> Result<Self, ProductRunnerError> {
        durable::decode_retained(bytes, transcript)
    }

    /// Restores exact retained state from a caller-owned streaming source.
    ///
    /// # Errors
    ///
    /// Rejects malformed, unsupported, or internally inconsistent retained state, including a
    /// source read error.
    pub fn decode_durable_retained_from(
        reader: impl std::io::Read,
        transcript: &str,
    ) -> Result<Self, ProductRunnerError> {
        durable::decode_retained_from(reader, transcript)
    }

    /// Reads only the version discriminator from a durable continuation source.
    ///
    /// # Errors
    ///
    /// Rejects a source that does not expose a well-formed version discriminator.
    pub fn durable_version_from(
        reader: impl std::io::Read,
    ) -> Result<u16, ProductRunnerError> {
        durable::version_from(reader)
    }

    /// Whether this product runner can decode a durable continuation version.
    #[must_use]
    pub const fn durable_version_supported(version: u16) -> bool {
        durable::version_supported(version)
    }

    /// Publishes retained legacy inline finding bodies before a daemon adopts a new record head.
    ///
    /// # Errors
    /// Rejects invalid encoded findings or any body publication failure.
    pub fn externalize_finding_bodies(
        &mut self,
        publisher: &dyn ProductFindingBodyPublisher,
    ) -> Result<bool, ProductRunnerError> {
        self.findings.externalize(publisher)
    }

    /// Whether every retained review artifact is represented by an immutable external reference.
    ///
    /// Encoded fallback state is deliberately treated as migration-pending: a host cannot attest
    /// an opaque representation merely because it can preserve its bytes.
    #[must_use]
    pub fn review_artifacts_externalized(&self) -> bool {
        self.findings.ledger().is_some_and(|ledger| {
            !ledger.has_inline_finding_bodies() && !ledger.has_inline_summary()
        })
    }

    /// Rebinds retained execution state to a host-reconciled candidate checkpoint.
    ///
    /// Source changes force a new design. Repository-only and execution-context changes retain
    /// the completed writer state and resume at deterministic checks.
    ///
    /// # Errors
    ///
    /// Rejects a checkpoint from another run or workspace, or a checkpoint sequence that moves
    /// backwards.
    pub fn reconcile_candidate(
        mut self,
        checkpoint: CandidateCheckpoint,
    ) -> Result<Self, ProductRunnerError> {
        let previous = *self.checkpoint.identity();
        let current = *checkpoint.identity();
        if previous.run_id() != current.run_id()
            || previous.workspace_id() != current.workspace_id()
            || current.checkpoint_sequence() < previous.checkpoint_sequence()
        {
            return Err(ProductRunnerError::new(
                ProductRunnerErrorKind::InvalidPrecondition,
                "reconcile retained product candidate",
                "host checkpoint does not continue the retained run lineage",
            ));
        }
        self.checkpoint = checkpoint;
        if !previous.same_content_and_requirements(&current) {
            self.next_phase = ProductRunPhase::Designing;
            self.gate_report = None;
            return Ok(self);
        }
        if !self
            .gate_report
            .as_ref()
            .is_some_and(|report| gate_state::report_is_current(report, &self.checkpoint))
        {
            self.gate_report = None;
        }
        self.next_phase = if self.design.is_none() {
            ProductRunPhase::Designing
        } else {
            phase_with_retained_evidence(
                self.next_phase,
                &self.checkpoint,
                self.gate_report.as_ref(),
            )
        };
        Ok(self)
    }

    pub(super) fn phase_with_current_checkpoint(
        &self,
        planned: ProductRunPhase,
        checkpoint: &CandidateCheckpoint,
    ) -> ProductRunPhase {
        if self.design.is_none() {
            ProductRunPhase::Designing
        } else {
            phase_with_retained_evidence(planned, checkpoint, self.gate_report.as_ref())
        }
    }

    pub(super) fn encode_retained_gate_state(
        &self,
    ) -> Result<Option<(Vec<u8>, peritus_types::Sha256Digest)>, ProductRunnerError> {
        self.gate_report
            .as_ref()
            .map(|report| {
                let state = gate_state::RetainedGateState::capture(report, &self.checkpoint)?;
                let digest = state.binding_digest();
                Ok((state.encode()?, digest))
            })
            .transpose()
    }

    pub(super) fn restore_retained_gate_state(
        &mut self,
        bytes: Option<&[u8]>,
        expected_digest: Option<peritus_types::Sha256Digest>,
    ) -> Result<(), ProductRunnerError> {
        self.gate_report = match (bytes, expected_digest) {
            (Some(bytes), Some(expected_digest)) => {
                let state = gate_state::RetainedGateState::decode(bytes)?;
                if state.binding_digest() != expected_digest {
                    return Err(ProductRunnerError::new(
                        ProductRunnerErrorKind::InvalidPrecondition,
                        "restore candidate-bound gate state",
                        "gate-state section digest differs from its continuation descriptor",
                    ));
                }
                Some(state.restore(&self.checkpoint)?)
            }
            (None, None) => None,
            (Some(_), None) | (None, Some(_)) => {
                return Err(ProductRunnerError::new(
                    ProductRunnerErrorKind::InvalidPrecondition,
                    "restore candidate-bound gate state",
                    "gate-state section bytes and descriptor must be present together",
                ));
            }
        };
        self.next_phase = if self.design.is_none() {
            ProductRunPhase::Designing
        } else {
            phase_with_retained_evidence(
                self.next_phase,
                &self.checkpoint,
                self.gate_report.as_ref(),
            )
        };
        Ok(())
    }

    #[cfg(test)]
    pub(super) fn design_markdown(&self) -> &str {
        self.design.as_ref().expect("strict capture retains a design").markdown()
    }

    pub(super) const fn design(&self) -> Option<&RetainedDesign> {
        self.design.as_ref()
    }

    pub(super) fn restored_design(&self) -> Option<DesignDocument> {
        self.design.as_ref().map(RetainedDesign::document)
    }

    pub(super) fn task_summary(&self) -> &str {
        &self.task_summary
    }

    pub(super) fn run_instructions(&self) -> &str {
        &self.run_instructions
    }

    pub(super) fn fix_summaries(&self) -> &[String] {
        &self.fix_summaries
    }

    pub(super) const fn tool_calls(&self) -> u64 {
        self.tool_calls
    }

    pub(super) const fn retained_findings(&self) -> &RetainedFindings {
        &self.findings
    }

    pub(super) fn finding_ledger(&self) -> Option<&ProductFindingLedger> {
        self.findings.ledger()
    }

    pub(super) fn developer_evidence(&self) -> &str {
        &self.developer_evidence
    }

    pub(super) fn diff(&self) -> &str {
        &self.diff
    }

    pub(super) fn gates(&self) -> &str {
        &self.gates
    }

    pub(super) fn review(&self) -> &str {
        &self.review
    }

    pub(super) const fn gate_report(&self) -> Option<&crate::gates::GateReport> {
        self.gate_report.as_ref()
    }

    pub(super) fn successful_commands(&self) -> &[SuccessfulCommand] {
        &self.successful_commands
    }

    pub(super) const fn fixer_cycles(&self) -> u32 {
        self.fixer_cycles
    }

    pub(super) const fn obligation_source_root(
        &self,
    ) -> Option<peritus_types::Sha256Digest> {
        self.obligation_source_root
    }

    pub(super) fn set_obligation_source_root(
        &mut self,
        root: Option<peritus_types::Sha256Digest>,
    ) {
        self.obligation_source_root = root;
    }
}

impl RetainedDesign {
    pub(super) fn new(
        path: PathBuf,
        markdown: String,
        conversation_revision: u64,
    ) -> Result<Self, ProductRunnerError> {
        if path.as_os_str().is_empty() || markdown.trim().is_empty() {
            return Err(ProductRunnerError::new(
                ProductRunnerErrorKind::InvalidPrecondition,
                "restore retained product design",
                "retained design path and content must be present together",
            ));
        }
        Ok(Self { path, markdown, conversation_revision })
    }

    pub(super) fn from_document(document: &DesignDocument) -> Self {
        Self {
            path: document.path().to_owned(),
            markdown: document.markdown().to_owned(),
            conversation_revision: document.conversation_revision(),
        }
    }

    pub(super) fn path(&self) -> &std::path::Path {
        &self.path
    }

    pub(super) fn markdown(&self) -> &str {
        &self.markdown
    }

    pub(super) const fn conversation_revision(&self) -> u64 {
        self.conversation_revision
    }

    fn document(&self) -> DesignDocument {
        DesignDocument::restored(
            self.path.clone(),
            self.markdown.clone(),
            self.conversation_revision,
        )
    }
}

impl RetainedFindings {
    pub(super) fn from_ledger(ledger: ProductFindingLedger) -> Self {
        Self::Typed(ledger)
    }

    pub(super) fn from_encoded(encoded: String) -> Self {
        match crate::review::restore_ledger(&encoded) {
            Ok(ledger) => Self::Typed(ledger),
            Err(_) => Self::Encoded(encoded),
        }
    }

    pub(super) const fn ledger(&self) -> Option<&ProductFindingLedger> {
        match self {
            Self::Typed(ledger) => Some(ledger),
            Self::Encoded(_) => None,
        }
    }

    pub(super) fn encoded(&self) -> Option<&str> {
        match self {
            Self::Typed(_) => None,
            Self::Encoded(encoded) => Some(encoded),
        }
    }

    fn knowledge_projection(&self) -> Result<String, ProductRunnerError> {
        match self {
            Self::Typed(ledger) => crate::review::encode_ledger(ledger),
            Self::Encoded(encoded) => {
                let ledger = crate::review::restore_ledger(encoded)?;
                crate::review::encode_ledger(&ledger)
            }
        }
    }

    fn externalize(
        &mut self,
        publisher: &dyn ProductFindingBodyPublisher,
    ) -> Result<bool, ProductRunnerError> {
        if let Self::Typed(ledger) = self {
            return ledger.externalize_bodies(publisher).map_err(|error| {
                ProductRunnerError::new(
                    ProductRunnerErrorKind::Repository,
                    "publish retained D2 finding body",
                    error.to_string(),
                )
            });
        }
        let Self::Encoded(encoded) = self else { unreachable!() };
        let mut ledger = crate::review::restore_ledger(encoded)?;
        let changed = ledger.externalize_bodies(publisher).map_err(|error| {
            ProductRunnerError::new(
                ProductRunnerErrorKind::Repository,
                "publish retained D2 finding body",
                error.to_string(),
            )
        })?;
        *self = Self::Typed(ledger);
        Ok(changed)
    }
}

fn phase_with_retained_evidence(
    requested: ProductRunPhase,
    checkpoint: &CandidateCheckpoint,
    gate_report: Option<&crate::gates::GateReport>,
) -> ProductRunPhase {
    let gate_current = gate_report
        .is_some_and(|report| gate_state::report_is_current(report, checkpoint));
    match requested {
        ProductRunPhase::Designing
        | ProductRunPhase::Writing
        | ProductRunPhase::Checking
        | ProductRunPhase::Verifying => requested,
        ProductRunPhase::Reviewing if gate_current => ProductRunPhase::Reviewing,
        ProductRunPhase::Fixing
            if gate_current
                && checkpoint.obligations().is_current_for(checkpoint.identity())
                && checkpoint.review().is_current_for(checkpoint.identity()) =>
        {
            ProductRunPhase::Fixing
        }
        ProductRunPhase::Finalizing | ProductRunPhase::Complete
            if gate_current && checkpoint.is_qualified() =>
        {
            ProductRunPhase::Finalizing
        }
        ProductRunPhase::Reviewing
        | ProductRunPhase::Fixing
        | ProductRunPhase::Finalizing
        | ProductRunPhase::Complete
            if gate_current =>
        {
            ProductRunPhase::Reviewing
        }
        ProductRunPhase::Reviewing
        | ProductRunPhase::Fixing
        | ProductRunPhase::Finalizing
        | ProductRunPhase::Complete => ProductRunPhase::Checking,
    }
}

fn missing_design() -> ProductRunnerError {
    ProductRunnerError::new(
        ProductRunnerErrorKind::InternalInvariant,
        "capture retained product continuation",
        "the exact candidate has no completed design; the same run must resume the design phase",
    )
}

fn combined_capture_diagnostic(
    mut diagnostics: Vec<ProductRunnerError>,
) -> Option<ProductRunnerError> {
    if diagnostics.len() == 1 {
        return diagnostics.pop();
    }
    if diagnostics.is_empty() {
        return None;
    }
    Some(ProductRunnerError::new(
        ProductRunnerErrorKind::InternalInvariant,
        "capture retained product continuation",
        diagnostics
            .iter()
            .map(ProductRunnerError::settlement_detail)
            .collect::<Vec<_>>()
            .join("; "),
    ))
}

#[cfg(test)]
mod tests;
