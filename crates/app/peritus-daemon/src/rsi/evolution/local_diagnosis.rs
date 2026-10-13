//! Deterministic observation-only E2 report producer over actual published native E3/C7 evidence.

use super::{incomplete, owner_error};
use peritus_artifact_store::ArtifactStore;
use peritus_debugger::{
    AnalysisSubject, ArtifactCitation, CausalClosure, DebuggerLimits, DebuggerReport,
    DiagnosticText, EvidenceCitation, ObservationFilter, ReportClaim, SelectedArtifact,
    TraceSelectionManifest, TraceSelectionQuery, ValidatedReport, build_timelines, select_evidence,
    summarize_health,
};
use peritus_eval::{EvaluationCampaignId, EvaluationPhase};
use peritus_evolution::EvolutionError;
use peritus_journal::SqliteJournal;
use peritus_trace::TraceId;
use peritus_types::EnvironmentId;

/// Actual immutable E2 input selection and validated observational report.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeDiagnosticReport {
    /// Frozen exact native E3/C7 query.
    pub query: TraceSelectionQuery,
    /// Actual C0-provenance checked C7 selection and ordinary source manifests.
    pub selection: TraceSelectionManifest,
    /// Actual deterministic report; carries no causal inference or promotion authority.
    pub report: ValidatedReport,
}

/// Produces an observation-only report from a fully published native comparison.
///
/// This helper reloads actual protected profile, ledger, report, native C7 manifests and traces.
/// It does not call a model, assign failure causes, approve an improvement, or assert eligibility.
///
/// # Errors
/// Rejects unpublished/drifted native evidence, missing genuine C7 manifests or malformed IDs.
pub fn local_diagnostic_report(
    journal: &mut SqliteJournal,
    artifacts: &ArtifactStore,
    campaign: EvaluationCampaignId,
) -> Result<NativeDiagnosticReport, EvolutionError> {
    let state = crate::rsi::evaluation::load(journal, campaign).map_err(owner_error)?;
    if state.phase() != EvaluationPhase::Published {
        return Err(incomplete("native diagnostic source is not published"));
    }
    let profile = crate::rsi::evaluation::profile(artifacts, &state).map_err(owner_error)?;
    let ledger =
        crate::rsi::evaluation::ledger(artifacts, &state, &profile).map_err(owner_error)?;
    let report = crate::rsi::evaluation::report(artifacts, &state).map_err(owner_error)?;
    let plan = peritus_eval::EvaluationPlan::build(campaign, &profile).map_err(owner_error)?;
    let mut subjects = Vec::new();
    let mut ordinary = Vec::new();
    let mut associations = Vec::new();
    for record in ledger.records() {
        let terminal = match state
            .rollout(record.rollout_id())
            .map(|value| value.status())
        {
            Some(peritus_eval::RolloutStatus::Settled(terminal)) => terminal,
            _ => {
                return Err(incomplete(
                    "native source terminal has no settled C0 reference",
                ));
            }
        };
        let terminal_metadata = artifacts.verify(terminal.artifact()).map_err(owner_error)?;
        ordinary
            .push(SelectedArtifact::from_metadata(&terminal_metadata, None).map_err(owner_error)?);
        let Some(manifest) = record.trace_root() else {
            let spec = plan
                .specs()
                .iter()
                .find(|value| value.id() == record.rollout_id())
                .ok_or_else(|| {
                    incomplete("native failure is absent from its actual frozen plan")
                })?;
            let observed = super::local_failure_trace::import_published_failure_trace(
                journal, artifacts, &profile, spec, &record,
            )?;
            let subject = AnalysisSubject::from_published_local_failure(
                journal,
                artifacts,
                &profile,
                &report,
                record.rollout_id(),
                observed.trace,
                observed.environment,
            )
            .map_err(owner_error)?;
            associations.push((
                subject.id(),
                record,
                terminal.artifact(),
                terminal_metadata.size(),
                "failure",
            ));
            subjects.push(subject);
            continue;
        };
        let metadata = artifacts.verify(manifest).map_err(owner_error)?;
        let bytes = artifacts
            .read(manifest, metadata.size())
            .map_err(owner_error)?;
        let document: serde_json::Value = serde_json::from_slice(&bytes).map_err(owner_error)?;
        for role in ["candidate", "evaluator"] {
            let stage = document
                .get(role)
                .ok_or_else(|| incomplete("native C7 manifest omits an actual process role"))?;
            let trace: [u8; 16] = serde_json::from_value(
                stage
                    .get("trace_id")
                    .cloned()
                    .ok_or_else(|| incomplete("native C7 trace identity absent"))?,
            )
            .map_err(owner_error)?;
            let environment: [u8; 16] = serde_json::from_value(
                stage
                    .get("environment_id")
                    .cloned()
                    .ok_or_else(|| incomplete("native C7 environment absent"))?,
            )
            .map_err(owner_error)?;
            let subject = AnalysisSubject::from_published_local_evaluation(
                journal,
                artifacts,
                &profile,
                &report,
                record.rollout_id(),
                TraceId::new(trace).map_err(owner_error)?,
                EnvironmentId::new(environment).map_err(owner_error)?,
            )
            .map_err(owner_error)?;
            associations.push((
                subject.id(),
                record,
                terminal.artifact(),
                terminal_metadata.size(),
                role,
            ));
            subjects.push(subject);
        }
    }
    subjects.sort_unstable_by_key(AnalysisSubject::id);
    ordinary.sort_unstable_by_key(SelectedArtifact::digest);
    ordinary.dedup_by_key(|value| value.digest());
    let limits = DebuggerLimits::COMPILED;
    let query = TraceSelectionQuery::new(
        subjects,
        None,
        None,
        ObservationFilter::new(None, None).map_err(owner_error)?,
        CausalClosure::IncludeAncestors,
        limits,
    )
    .map_err(owner_error)?;
    let traces = peritus_trace::recover_all(journal).map_err(owner_error)?;
    let export = journal.integrity_export().map_err(owner_error)?;
    let selection = select_evidence(&query, &traces, &export, ordinary).map_err(owner_error)?;
    let timelines = build_timelines(&selection, limits).map_err(owner_error)?;
    let mut claims = Vec::new();
    for (subject, record, manifest, size, role) in associations {
        let entry = selection
            .entries()
            .iter()
            .filter(|entry| entry.subject().id() == subject)
            .max_by_key(|entry| entry.journal_position())
            .ok_or_else(|| incomplete("native subject has no actual selected C7 observations"))?;
        let citation = EvidenceCitation::new(
            &selection,
            subject,
            entry.event_id(),
            entry.journal_position(),
            entry.frame_digest(),
            Some(ArtifactCitation::new(manifest, 0, size).map_err(owner_error)?),
        )
        .map_err(owner_error)?;
        let statement = if role == "failure" {
            DiagnosticText::new(format!("Native E3 retained {:?} for task {:?}, arm {:?}, ordinal {}; the cited canonical C0 terminal is observed directly. Missing process traces and unavailable outputs or measurements remain unavailable.",record.outcome(),record.task_id(),record.arm(),record.ordinal()))
        } else {
            DiagnosticText::new(format!("Native E3 sealed evaluator recorded {:?} for task {:?}, arm {:?}, ordinal {}; the cited canonical terminal record binds the protected manifest of actual {role} C2 output and teardown observations.",record.outcome(),record.task_id(),record.arm(),record.ordinal()))
        }.map_err(owner_error)?;
        claims.push(ReportClaim::observation(statement, vec![citation]).map_err(owner_error)?);
    }
    claims.sort_unstable_by_key(ReportClaim::id);
    let health = summarize_health(&selection, &[], &[], &[], &[], limits).map_err(owner_error)?;
    let validated = DebuggerReport::new(
        &selection,
        None,
        timelines,
        vec![],
        vec![],
        vec![],
        health,
        claims,
    )
    .validate(&selection, limits)
    .map_err(owner_error)?;
    Ok(NativeDiagnosticReport {
        query,
        selection,
        report: validated,
    })
}
