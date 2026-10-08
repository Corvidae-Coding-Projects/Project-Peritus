//! Restorable product-run state and initial design/writer phase preparation.

use peritus_orchestrator::ProductionRunCoordinator;
use peritus_review::ProductFindingLedger;
use peritus_run_settlement::CandidateStage;

use super::{
    AppliedTurn, AppliedWrite, ProductRunInput, ProductRunPhase, RunObserver,
    checkpoint::{CandidateRecorder, CheckpointEvidence},
    cycle::{RunEvidence, create_design, initial_write},
    obligations::RunObligations,
    resume::ProductRunResume,
};
use crate::{
    ProductRunnerError, ProductRunnerErrorKind, ProductRunnerFailureCause, budget::RunAccounting,
    candidate::CandidateBaseline, design, developer_tools::WorkspaceOwnership, gates, review,
};

pub(super) struct ExecutionContext {
    pub(super) baseline: CandidateBaseline,
    pub(super) recorder: CandidateRecorder,
    pub(super) obligations: RunObligations,
    pub(super) design: Option<design::DesignDocument>,
    pub(super) state: Option<RunState>,
    pub(super) evidence: RunEvidence,
    pub(super) gate_report: Option<gates::GateReport>,
    pub(super) next_phase: ProductRunPhase,
}

impl ExecutionContext {
    pub(super) fn prepare(input: &ProductRunInput) -> Result<Self, ProductRunnerError> {
        Self::prepare_inner(input, false)
    }

    pub(super) fn prepare_for_finalization(
        input: &ProductRunInput,
    ) -> Result<Self, ProductRunnerError> {
        Self::prepare_inner(input, true)
    }

    fn prepare_inner(
        input: &ProductRunInput,
        for_finalization: bool,
    ) -> Result<Self, ProductRunnerError> {
        if let Some(resume) = &input.resume
            && resume.baseline().scope() != input.in_place_scope().as_ref()
        {
            return Err(ProductRunnerError::new(
                ProductRunnerErrorKind::InvalidPrecondition,
                "restore workspace delivery",
                "resume delivery scope differs from the caller's current workspace capability",
            ));
        }
        let (obligations, transcript) = loop {
            let obligations = RunObligations::capture_input(input)?;
            let transcript = input.conversation.render();
            if obligations.source_contract_is_current(input) {
                break (obligations, transcript);
            }
            if for_finalization {
                super::cancellation::check_finalization_cancelled(input)?;
            } else {
                super::check_cancelled(input)?;
            }
        };
        let conversation_revision = obligations
            .source_revision()
            .unwrap_or_else(|| input.conversation.revision());
        let baseline = input
            .resume
            .as_ref()
            .map_or_else(|| input.baseline(), |resume| Ok(resume.baseline().clone()))?;
        let prior = input.resume.as_ref().map(ProductRunResume::checkpoint);
        let recorder = CandidateRecorder::for_input(input, baseline.clone(), prior)?;
        let obligation_contract_changed = input.resume.as_ref().map_or_else(
            || obligations.source_root_digest().is_some(),
            |resume| resume.obligation_source_root() != obligations.source_root_digest(),
        );
        if obligation_contract_changed {
            if for_finalization {
                recorder.adopt_obligation_contract_for_finalization(conversation_revision)?;
            } else {
                recorder.adopt_obligation_contract(conversation_revision)?;
            }
        } else if for_finalization {
            let _ = recorder.refresh_for_finalization(conversation_revision)?;
        } else {
            let _ = recorder.refresh(conversation_revision)?;
        }
        let next_phase = match (&input.resume, recorder.checkpoint()?) {
            (Some(resume), Some(checkpoint)) => {
                let planned = resume.plan(*checkpoint.identity(), &transcript)?;
                resume.phase_with_current_checkpoint(planned, &checkpoint)
            }
            _ => ProductRunPhase::Designing,
        };
        let (design, mut state, evidence, gate_report) = if let Some(resume) = &input.resume {
            let design = resume.restored_design();
            if next_phase == ProductRunPhase::Designing {
                // The prior design is not reusable for execution after a conversation change,
                // but it remains the last durable design artifact for the unchanged workspace
                // candidate. Retain it only as a finalization fallback until create_design
                // replaces it; prior evidence is deliberately not carried forward.
                (design, None, RunEvidence::default(), None)
            } else {
                let evidence = RunEvidence {
                    diff: resume.diff().to_owned(),
                    gates: resume.gates().to_owned(),
                    review: resume.review().to_owned(),
                    developer_commands: resume.developer_evidence().to_owned(),
                };
                let state = (next_phase != ProductRunPhase::Writing)
                    .then(|| {
                        let design = design.clone().ok_or_else(missing_design)?;
                        RunState::restore(resume, design)
                    })
                    .transpose()?;
                (design, state, evidence, resume.gate_report().cloned())
            }
        } else {
            (None, None, RunEvidence::default(), None)
        };
        if let Some(state) = &mut state {
            externalize_findings(input, &mut state.findings)?;
        }
        Ok(Self {
            baseline,
            recorder,
            obligations,
            design,
            state,
            evidence,
            gate_report,
            next_phase,
        })
    }

    /// Adopts a changed authoritative source root before any phase reuses prior qualification.
    /// Physical page reconstruction remains cancellation-aware and has no lifetime attempt cap.
    pub(super) fn refresh_obligation_contract(
        &mut self,
        input: &ProductRunInput,
    ) -> Result<bool, ProductRunnerError> {
        self.refresh_obligation_contract_inner(input, false)
    }

    pub(super) fn refresh_obligation_contract_for_finalization(
        &mut self,
        input: &ProductRunInput,
    ) -> Result<bool, ProductRunnerError> {
        self.refresh_obligation_contract_inner(input, true)
    }

    fn refresh_obligation_contract_inner(
        &mut self,
        input: &ProductRunInput,
        for_finalization: bool,
    ) -> Result<bool, ProductRunnerError> {
        if self.obligations.source_contract_is_current(input) {
            return Ok(false);
        }
        let replacement = loop {
            let replacement = RunObligations::capture_input(input)?;
            if replacement.source_contract_is_current(input) {
                break replacement;
            }
            if for_finalization {
                super::cancellation::check_finalization_cancelled(input)?;
            } else {
                super::check_cancelled(input)?;
            }
        };
        let changed = replacement.source_root_digest() != self.obligations.source_root_digest();
        if changed {
            let revision = replacement
                .source_revision()
                .unwrap_or_else(|| input.conversation.revision());
            if for_finalization {
                self.recorder.adopt_obligation_contract_for_finalization(revision)?;
            } else {
                self.recorder.adopt_obligation_contract(revision)?;
            }
            self.next_phase = ProductRunPhase::Designing;
        }
        self.obligations = replacement;
        Ok(changed)
    }

    pub(super) async fn prepare_active_state(
        &mut self,
        input: &ProductRunInput,
        observe: &RunObserver,
        ownership: &mut WorkspaceOwnership,
        accounting: &mut RunAccounting,
    ) -> Result<Option<(String, u64)>, ProductRunnerError> {
        if self.next_phase == ProductRunPhase::Designing {
            let candidate = *self
                .recorder
                .checkpoint()?
                .ok_or_else(|| {
                    ProductRunnerError::new(
                        ProductRunnerErrorKind::InternalInvariant,
                        "start design phase",
                        "design phase has no retained candidate identity",
                    )
                })?
                .identity();
            self.design = Some(create_design(input, observe, 1, accounting, candidate).await?);
            self.next_phase = ProductRunPhase::Writing;
        }
        if self.next_phase != ProductRunPhase::Writing {
            return Ok(None);
        }
        let design = self.design.as_ref().ok_or_else(|| {
            ProductRunnerError::new(
                ProductRunnerErrorKind::InternalInvariant,
                "start writer phase",
                "writer phase has no current implementation design",
            )
        })?;
        let mut restored_findings = match input.resume.as_ref() {
            Some(resume) => resume.finding_ledger().cloned().ok_or_else(missing_findings)?,
            None => review::restore_ledger(&input.finding_state)?,
        };
        externalize_findings(input, &mut restored_findings)?;
        let prior_findings =
            (restored_findings.cycle() > 0).then(|| review::render(&restored_findings));
        let applied = match initial_write(
            input,
            observe,
            design.markdown(),
            prior_findings.as_deref(),
            ownership,
            accounting,
            &self.recorder,
        )
        .await?
        {
            AppliedTurn::Applied(applied) => applied,
            AppliedTurn::Waiting { question, conversation_revision, host } => {
                self.state = Some(RunState::interrupted(
                    input,
                    design.clone(),
                    restored_findings,
                    host,
                )?);
                return Ok(Some((question, conversation_revision)));
            }
            AppliedTurn::Rejected { error, host } => {
                self.state = Some(RunState::interrupted(
                    input,
                    design.clone(),
                    restored_findings,
                    host,
                )?);
                return Err(error);
            }
        };
        let stage =
            if applied.successful_commands.iter().any(|command| {
                command.purpose == crate::developer_tools::CommandPurpose::Verification
            }) {
                CandidateStage::SelfChecked
            } else {
                CandidateStage::Changed
            };
        let _ =
            self.recorder.record(stage, applied.conversation_revision, CheckpointEvidence::None)?;
        let mut run_state = RunState::new(design.clone(), restored_findings, applied);
        if let Some(resume) = &input.resume {
            run_state.merge_resume_host(resume)?;
        }
        self.state = Some(run_state);
        self.next_phase = ProductRunPhase::Checking;
        Ok(None)
    }

    pub(super) fn completed_cycles(&self) -> u32 {
        self.state.as_ref().map_or(0, |state| state.coordinator.completed_fixer_cycles())
    }
}

fn externalize_findings(
    input: &ProductRunInput,
    findings: &mut ProductFindingLedger,
) -> Result<(), ProductRunnerError> {
    let Some(publisher) = input.conversation.finding_body_publisher() else { return Ok(()) };
    let changed = findings.externalize_bodies(publisher).map_err(|error| {
        ProductRunnerError::new(
            ProductRunnerErrorKind::Repository,
            "publish retained D2 finding bodies",
            error.to_string(),
        )
        .with_failure_cause(ProductRunnerFailureCause::ContextPreparation)
    })?;
    if changed {
        let encoded = review::encode_ledger(findings)?;
        input.conversation.adopt_finding_state(&encoded).map_err(|error| {
            ProductRunnerError::new(
                ProductRunnerErrorKind::Repository,
                "adopt retained D2 finding body descriptors",
                error,
            )
            .with_failure_cause(ProductRunnerFailureCause::ContextPreparation)
        })?;
    }
    Ok(())
}

pub(super) struct RunState {
    pub(super) task_summary: String,
    pub(super) run_instructions: String,
    pub(super) design: design::DesignDocument,
    pub(super) fix_summaries: Vec<String>,
    pub(super) tool_calls: u64,
    pub(super) conversation_revision: u64,
    pub(super) findings: ProductFindingLedger,
    pub(super) coordinator: ProductionRunCoordinator,
    pub(super) developer_evidence: String,
    pub(super) successful_commands: Vec<crate::developer_tools::SuccessfulCommand>,
}

impl RunState {
    fn new(
        design: design::DesignDocument,
        findings: ProductFindingLedger,
        applied: AppliedWrite,
    ) -> Self {
        Self {
            task_summary: applied.summary,
            run_instructions: applied.run_instructions,
            design,
            fix_summaries: Vec::new(),
            tool_calls: applied.tool_calls,
            conversation_revision: applied.conversation_revision,
            findings,
            coordinator: ProductionRunCoordinator::new(0),
            developer_evidence: applied.verification_evidence,
            successful_commands: applied.successful_commands,
        }
    }

    fn restore(
        resume: &ProductRunResume,
        design: design::DesignDocument,
    ) -> Result<Self, ProductRunnerError> {
        Ok(Self {
            task_summary: resume.task_summary().to_owned(),
            run_instructions: resume.run_instructions().to_owned(),
            design,
            fix_summaries: resume.fix_summaries().to_vec(),
            tool_calls: resume.tool_calls(),
            conversation_revision: resume.checkpoint().identity().requirements_revision(),
            findings: resume.finding_ledger().cloned().ok_or_else(missing_findings)?,
            coordinator: ProductionRunCoordinator::new(resume.fixer_cycles()),
            developer_evidence: resume.developer_evidence().to_owned(),
            successful_commands: resume.successful_commands().to_vec(),
        })
    }

    pub(super) fn interrupted(
        input: &ProductRunInput,
        design: design::DesignDocument,
        findings: ProductFindingLedger,
        host: super::HostTurnEvidence,
    ) -> Result<Self, ProductRunnerError> {
        let mut state = Self {
            task_summary: "Peritus retained host-observed work after the developer's terminal report could not be accepted."
                .to_owned(),
            run_instructions: "Resume this Peritus run to complete verification and acceptance."
                .to_owned(),
            design,
            fix_summaries: Vec::new(),
            tool_calls: host.tool_calls,
            conversation_revision: host.conversation_revision,
            findings,
            coordinator: ProductionRunCoordinator::new(0),
            developer_evidence: host.verification_evidence,
            successful_commands: host.successful_commands,
        };
        if let Some(resume) = &input.resume {
            state.merge_resume_host(resume)?;
        }
        Ok(state)
    }

    pub(super) fn merge_host(
        &mut self,
        host: &super::HostTurnEvidence,
    ) -> Result<(), ProductRunnerError> {
        self.tool_calls = self.tool_calls.checked_add(host.tool_calls).ok_or_else(|| {
            ProductRunnerError::new(
                ProductRunnerErrorKind::InternalInvariant,
                "accumulate developer tool calls",
                "developer tool-call counter overflow",
            )
        })?;
        self.conversation_revision = host.conversation_revision;
        crate::developer_tools::merge_rendered(
            &mut self.developer_evidence,
            &host.verification_evidence,
        );
        crate::developer_tools::merge_successful(
            &mut self.successful_commands,
            &host.successful_commands,
        );
        Ok(())
    }

    fn merge_resume_host(&mut self, resume: &ProductRunResume) -> Result<(), ProductRunnerError> {
        self.tool_calls = self.tool_calls.checked_add(resume.tool_calls()).ok_or_else(|| {
            ProductRunnerError::new(
                ProductRunnerErrorKind::InternalInvariant,
                "accumulate retained developer tool calls",
                "retained developer tool-call counter overflow",
            )
        })?;
        let current_evidence = std::mem::take(&mut self.developer_evidence);
        resume.developer_evidence().clone_into(&mut self.developer_evidence);
        crate::developer_tools::merge_rendered(&mut self.developer_evidence, &current_evidence);
        let mut retained = resume.successful_commands().to_vec();
        for command in std::mem::take(&mut self.successful_commands) {
            if !retained.contains(&command) {
                retained.push(command);
            }
        }
        self.successful_commands = retained;
        Ok(())
    }
}

fn missing_design() -> ProductRunnerError {
    ProductRunnerError::new(
        ProductRunnerErrorKind::InternalInvariant,
        "restore retained product state",
        "the retained run has no completed design for this active phase",
    )
    .with_failure_cause(ProductRunnerFailureCause::ContextPreparation)
}

fn missing_findings() -> ProductRunnerError {
    ProductRunnerError::new(
        ProductRunnerErrorKind::InternalInvariant,
        "restore retained product state",
        "the retained run has no decoded authoritative finding ledger",
    )
    .with_failure_cause(ProductRunnerFailureCause::ContextPreparation)
}
