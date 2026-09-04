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
    ProductRunnerError, ProductRunnerErrorKind, budget::RunAccounting,
    candidate::CandidateBaseline, design, developer_tools::WorkspaceOwnership, gates, review,
};

const MAX_FIX_CYCLES: u32 = 8;

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
        let transcript = input.conversation.render();
        let obligations = RunObligations::capture(&transcript, input.conversation.revision())?;
        let baseline = input.resume.as_ref().map_or_else(
            || CandidateBaseline::capture(&input.workspace_root),
            |resume| Ok(resume.baseline().clone()),
        )?;
        let prior = input.resume.as_ref().map(ProductRunResume::checkpoint);
        let recorder = CandidateRecorder::new(
            &input.workspace_root,
            baseline.clone(),
            input.run_id,
            input.workspace_id,
            prior,
            input.delivery_scope.allows_external_effects(),
        )?;
        let _ = recorder.refresh(input.conversation.revision())?;
        let next_phase = match (&input.resume, recorder.checkpoint()?) {
            (Some(resume), Some(checkpoint)) => resume.plan(*checkpoint.identity(), &transcript)?,
            _ => ProductRunPhase::Designing,
        };
        let (design, state, evidence, gate_report) = if let Some(resume) = &input.resume {
            let design = design::DesignDocument::restored(
                resume.design_path().clone(),
                resume.design_markdown().to_owned(),
                resume.design_revision(),
            );
            if next_phase == ProductRunPhase::Designing {
                // The prior design is not reusable for execution after a conversation change,
                // but it remains the last durable design artifact for the unchanged workspace
                // candidate. Retain it only as a finalization fallback until create_design
                // replaces it; prior evidence is deliberately not carried forward.
                (Some(design), None, RunEvidence::default(), None)
            } else {
                let evidence = RunEvidence {
                    diff: resume.diff().to_owned(),
                    gates: resume.gates().to_owned(),
                    review: resume.review().to_owned(),
                    developer_commands: resume.developer_evidence().to_owned(),
                };
                let state = (next_phase != ProductRunPhase::Writing)
                    .then(|| RunState::restore(input, resume, design.clone()))
                    .transpose()?;
                (Some(design), state, evidence, resume.gate_report().cloned())
            }
        } else {
            (None, None, RunEvidence::default(), None)
        };
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

    pub(super) async fn prepare_active_state(
        &mut self,
        input: &ProductRunInput,
        observe: &RunObserver,
        ownership: &mut WorkspaceOwnership,
        accounting: &mut RunAccounting,
    ) -> Result<Option<(String, u64)>, ProductRunnerError> {
        if self.next_phase == ProductRunPhase::Designing {
            self.design = Some(create_design(input, observe, 1, accounting).await?);
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
        let restored_findings = review::restore_ledger(
            input.resume.as_ref().map_or(&input.finding_state, |resume| resume.finding_state()),
        )?;
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
            AppliedTurn::Waiting { question, conversation_revision } => {
                return Ok(Some((question, conversation_revision)));
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
        self.state = Some(RunState::new(input, design.clone(), restored_findings, applied)?);
        self.next_phase = ProductRunPhase::Checking;
        Ok(None)
    }

    pub(super) fn completed_cycles(&self) -> u32 {
        self.state.as_ref().map_or(0, |state| state.coordinator.completed_fixer_cycles())
    }
}

pub(super) struct RunState {
    pub(super) task_summary: String,
    pub(super) run_instructions: String,
    pub(super) design: design::DesignDocument,
    pub(super) fix_summaries: Vec<String>,
    pub(super) tool_calls: u32,
    pub(super) conversation_revision: u64,
    pub(super) findings: ProductFindingLedger,
    pub(super) fix_progress: crate::execution::fix_progress::FixProgress,
    pub(super) coordinator: ProductionRunCoordinator,
    pub(super) developer_evidence: String,
    pub(super) successful_commands: Vec<crate::developer_tools::SuccessfulCommand>,
}

impl RunState {
    fn new(
        input: &ProductRunInput,
        design: design::DesignDocument,
        findings: ProductFindingLedger,
        applied: AppliedWrite,
    ) -> Result<Self, ProductRunnerError> {
        Ok(Self {
            task_summary: applied.summary,
            run_instructions: applied.run_instructions,
            design,
            fix_summaries: Vec::new(),
            tool_calls: applied.tool_calls,
            conversation_revision: applied.conversation_revision,
            findings,
            fix_progress: crate::execution::fix_progress::FixProgress::capture(
                &input.workspace_root,
            )?,
            coordinator: coordinator(0)?,
            developer_evidence: applied.verification_evidence,
            successful_commands: applied.successful_commands,
        })
    }

    fn restore(
        input: &ProductRunInput,
        resume: &ProductRunResume,
        design: design::DesignDocument,
    ) -> Result<Self, ProductRunnerError> {
        Ok(Self {
            task_summary: resume.task_summary().to_owned(),
            run_instructions: resume.run_instructions().to_owned(),
            design,
            fix_summaries: resume.fix_summaries().to_vec(),
            tool_calls: resume.tool_calls(),
            conversation_revision: resume.checkpoint().identity().conversation_revision(),
            findings: review::restore_ledger(resume.finding_state())?,
            fix_progress: crate::execution::fix_progress::FixProgress::capture(
                &input.workspace_root,
            )?,
            coordinator: coordinator(resume.fixer_cycles())?,
            developer_evidence: resume.developer_evidence().to_owned(),
            successful_commands: resume.successful_commands().to_vec(),
        })
    }
}

fn coordinator(completed_cycles: u32) -> Result<ProductionRunCoordinator, ProductRunnerError> {
    let mut coordinator = ProductionRunCoordinator::new(MAX_FIX_CYCLES).map_err(|detail| {
        ProductRunnerError::new(
            ProductRunnerErrorKind::InternalInvariant,
            "start E0 production coordinator",
            detail,
        )
    })?;
    for _ in 0..completed_cycles {
        coordinator.record_fixer_completed();
    }
    Ok(coordinator)
}
