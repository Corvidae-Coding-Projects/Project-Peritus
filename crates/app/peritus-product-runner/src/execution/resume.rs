//! Digest-valid phase resume planning over retained product-run state.

use std::path::PathBuf;

use peritus_run_settlement::CandidateCheckpoint;

mod durable;
mod hashing;
mod knowledge;

use knowledge::RoleKnowledge;

use super::ProductRunPhase;
use crate::{ProductRunnerError, candidate::CandidateBaseline, developer_tools::SuccessfulCommand};

/// Opaque retained state sufficient to continue at the first stale or missing phase.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProductRunResume {
    checkpoint: CandidateCheckpoint,
    baseline: CandidateBaseline,
    next_phase: ProductRunPhase,
    design_path: PathBuf,
    design_markdown: String,
    design_revision: u64,
    task_summary: String,
    run_instructions: String,
    fix_summaries: Vec<String>,
    tool_calls: u32,
    finding_state: String,
    diff: String,
    gates: String,
    review: String,
    gate_report: Option<crate::gates::GateReport>,
    developer_evidence: String,
    successful_commands: Vec<SuccessfulCommand>,
    fixer_cycles: u32,
    knowledge: RoleKnowledge,
}

/// Complete retained execution values copied into a resume handoff.
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
    pub(super) tool_calls: u32,
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

impl ProductRunResume {
    pub(super) fn capture(values: ResumeCapture) -> Result<Self, ProductRunnerError> {
        let knowledge = RoleKnowledge::capture(
            *values.checkpoint.identity(),
            &values.transcript,
            &values.design_markdown,
            &values.finding_state,
            &values.developer_evidence,
        )?;
        Ok(Self {
            checkpoint: values.checkpoint,
            baseline: values.baseline,
            next_phase: values.next_phase,
            design_path: values.design_path,
            design_markdown: values.design_markdown,
            design_revision: values.design_revision,
            task_summary: values.task_summary,
            run_instructions: values.run_instructions,
            fix_summaries: values.fix_summaries,
            tool_calls: values.tool_calls,
            finding_state: values.finding_state,
            diff: values.diff,
            gates: values.gates,
            review: values.review,
            gate_report: values.gate_report,
            developer_evidence: values.developer_evidence,
            successful_commands: values.successful_commands,
            fixer_cycles: values.fixer_cycles,
            knowledge,
        })
    }

    /// Exact candidate checkpoint at the interruption boundary.
    #[must_use]
    pub const fn checkpoint(&self) -> &CandidateCheckpoint {
        &self.checkpoint
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
    /// Returns an internal serialization error if the bounded continuation cannot be encoded.
    pub fn encode_durable(&self) -> Result<Vec<u8>, ProductRunnerError> {
        durable::encode(self)
    }

    /// Restores an opaque continuation from a versioned durable payload and current conversation.
    ///
    /// # Errors
    ///
    /// Rejects malformed, unsupported, or internally inconsistent retained state.
    pub fn decode_durable(bytes: &[u8], transcript: &str) -> Result<Self, ProductRunnerError> {
        durable::decode(bytes, transcript)
    }

    pub(super) const fn design_path(&self) -> &PathBuf {
        &self.design_path
    }

    pub(super) fn design_markdown(&self) -> &str {
        &self.design_markdown
    }

    pub(super) const fn design_revision(&self) -> u64 {
        self.design_revision
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

    pub(super) const fn tool_calls(&self) -> u32 {
        self.tool_calls
    }

    pub(super) fn finding_state(&self) -> &str {
        &self.finding_state
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
}

#[cfg(test)]
mod tests;
