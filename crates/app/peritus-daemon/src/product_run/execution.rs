//! Active product-run task ownership and terminal projection.
mod runtime;

use peritus_app_protocol::{
    ProductConversationRole, ProductDeliverable, ProductRunPhase, ProductRunSnapshot,
};
use peritus_product_runner::ProductRunOutcome;
use peritus_product_runner::control::GoalSettlement;
use peritus_run_settlement::RunDisposition;
use peritus_types::RunId;

use super::persistence::persist_record;
use super::snapshot::replace_snapshot;
use super::{ProductRunService, ProductRunServiceError};
mod goal;
mod launch;

impl ProductRunService {
    fn observe(&self, run_id: RunId, update: peritus_product_runner::ProductRunUpdate) {
        let Ok(mut records) = self.inner.records.write() else { return };
        let Some(record) = records.get_mut(&run_id) else { return };
        if record.snapshot.phase() == ProductRunPhase::RecoveryRequired {
            return;
        }
        if !update.finding_state.is_empty() {
            record.finding_state = update.finding_state;
        }
        record.progress.observe(update.progress);
        if let Some(checkpoint) = update.checkpoint {
            record.checkpoint = Some(checkpoint);
        }
        if !update.remaining_work.is_empty() {
            record.remaining_work = update.remaining_work;
        }
        let phase = match update.phase {
            peritus_product_runner::ProductRunPhase::Designing => ProductRunPhase::Designing,
            peritus_product_runner::ProductRunPhase::Writing => ProductRunPhase::Writing,
            peritus_product_runner::ProductRunPhase::Checking => ProductRunPhase::Checking,
            peritus_product_runner::ProductRunPhase::Reviewing => ProductRunPhase::Reviewing,
            peritus_product_runner::ProductRunPhase::Fixing => ProductRunPhase::Fixing,
            peritus_product_runner::ProductRunPhase::Verifying => ProductRunPhase::Verifying,
            peritus_product_runner::ProductRunPhase::Finalizing => ProductRunPhase::Verifying,
            peritus_product_runner::ProductRunPhase::Complete => ProductRunPhase::Complete,
        };
        if phase != record.snapshot.phase()
            && update.phase != peritus_product_runner::ProductRunPhase::Finalizing
            && let Some(options) = &mut record.interaction
            && matches!(
                options.mode,
                peritus_app_protocol::ProductInteractionMode::Build
                    | peritus_app_protocol::ProductInteractionMode::Chat
            )
        {
            let message = match phase {
                ProductRunPhase::Designing => {
                    "I'm inspecting the workspace and preparing the design."
                }
                ProductRunPhase::Writing => "I'm moving on to the implementation.",
                ProductRunPhase::Checking => {
                    "The changes are ready for checks. I'm verifying them now."
                }
                ProductRunPhase::Reviewing => {
                    "The candidate is ready for independent review. I'll check it against your request."
                }
                ProductRunPhase::Fixing => {
                    "The review found issues to address. I'm working through those fixes."
                }
                ProductRunPhase::Verifying => {
                    "I'm checking the final result before handing it back to you."
                }
                _ => "",
            };
            if !message.is_empty()
                && options
                    .append(peritus_app_protocol::ProductActivityKind::Status, message, "")
                    .is_err()
            {
                options.persistence_failed.store(true, std::sync::atomic::Ordering::Release);
                return;
            }
        }
        if let Ok(snapshot) = ProductRunSnapshot::new(
            run_id,
            record.request.workspace_id(),
            record.request.providers(),
            phase,
            update.cycle,
            record.request.task().to_owned(),
            update.status,
            update.diff,
            update.gates,
            update.review,
            update.summary,
        ) {
            record.snapshot = snapshot;
            let _ = persist_record(&self.inner.directory, record);
        }
    }

    fn finish(
        &self,
        run_id: RunId,
        result: Result<ProductRunOutcome, peritus_product_runner::ProductRunnerError>,
    ) {
        let Ok(mut records) = self.inner.records.write() else { return };
        let Some(record) = records.get_mut(&run_id) else { return };
        if record.snapshot.phase() == ProductRunPhase::RecoveryRequired {
            let _ = persist_record(&self.inner.directory, record);
            return;
        }
        if let Ok(outcome) = &result {
            record.checkpoint = outcome.settlement().checkpoint().copied();
            record.settlement = Some(*outcome.settlement());
            record.resume = outcome.resume().cloned();
            record.remaining_work = outcome.remaining_work().to_vec();
            record.interruption_cause = outcome.detail().map_or_else(String::new, str::to_owned);
            record.candidate_actionable =
                !self.inner.folders.contains_key(&record.request.workspace_id())
                    && outcome.candidate().is_some()
                    && outcome.settlement().checkpoint().is_some();
            if matches!(
                outcome.settlement().disposition(),
                RunDisposition::Accepted | RunDisposition::WaitingForUser
            ) && let Some(start) =
                record.interaction.as_ref().and_then(|options| options.workbench.as_ref())
            {
                // The runner has returned, so its in-place writes have reached a completed owned
                // boundary. A failed seal remains visibly unsealed and can never be overwritten.
                let _ = self.seal_latest_checkpoint(start, run_id);
            }
        }
        if self.settle_workbench_goal(record, &result).is_err() {
            if let Some(options) = &record.interaction {
                options.persistence_failed.store(true, std::sync::atomic::Ordering::Release);
            }
            if let Ok(snapshot) = replace_snapshot(
                &record.snapshot,
                ProductRunPhase::RecoveryRequired,
                "Goal settlement persistence failed",
                "The runner stopped, but its terminal goal evidence could not be durably reconciled.",
            ) {
                record.snapshot = snapshot;
            }
            let _ = persist_record(&self.inner.directory, record);
            return;
        }
        let public_reply = match result {
            Ok(outcome) if outcome.settlement().disposition() == RunDisposition::Accepted => {
                let Some(output) = outcome.candidate() else {
                    fail_handoff(record);
                    let _ = persist_record(&self.inner.directory, record);
                    return;
                };
                let completion_message = format!("Completed: {}", output.summary);
                let deliverable = self.project_deliverable(record, &outcome);
                if deliverable.is_none()
                    && !self.inner.folders.contains_key(&record.request.workspace_id())
                {
                    fail_handoff(record);
                    let _ = persist_record(&self.inner.directory, record);
                    return;
                }
                if let Ok(snapshot) = ProductRunSnapshot::new(
                    run_id,
                    record.request.workspace_id(),
                    record.request.providers(),
                    ProductRunPhase::Complete,
                    output.fixer_cycles + 1,
                    record.request.task().to_owned(),
                    if deliverable.is_some() {
                        "Accepted — passing checks and independent review".to_owned()
                    } else {
                        "Completed in place — tracked task files passed checks and independent review".to_owned()
                    },
                    output.diff.clone(),
                    output.gates.clone(),
                    output.review.clone(),
                    output.summary.clone(),
                ) {
                    record.snapshot = match deliverable {
                        Some(deliverable) => snapshot.with_deliverable(deliverable),
                        None => snapshot,
                    };
                } else {
                    fail_handoff(record);
                    let _ = persist_record(&self.inner.directory, record);
                    return;
                }
                completion_message
            }
            Ok(outcome) if outcome.settlement().disposition() == RunDisposition::WaitingForUser => {
                let Some(question) = outcome.question() else {
                    fail_handoff(record);
                    let _ = persist_record(&self.inner.directory, record);
                    return;
                };
                let chatting = record.interaction.as_ref().is_some_and(|interaction| {
                    interaction.mode != peritus_app_protocol::ProductInteractionMode::Build
                });
                let status = if chatting {
                    if outcome.candidate().is_some() {
                        "Idle — unqualified changes retained"
                    } else {
                        "Idle — ready for your next message"
                    }
                } else if outcome.candidate().is_some() {
                    "Waiting for you — candidate preserved"
                } else {
                    "Waiting for you"
                };
                if let Ok(snapshot) = replace_snapshot(
                    &record.snapshot,
                    ProductRunPhase::WaitingForUser,
                    status,
                    &terminal_summary(&outcome, question.message()),
                ) {
                    record.snapshot = self.with_candidate(record, &outcome, snapshot);
                }
                question.message().to_owned()
            }
            Ok(outcome) => self.finish_nonaccepted(record, &outcome),
            Err(error) => {
                let phase =
                    if error.kind() == peritus_product_runner::ProductRunnerErrorKind::Cancelled {
                        ProductRunPhase::Cancelled
                    } else {
                        ProductRunPhase::Failed
                    };
                if let Ok(snapshot) = replace_snapshot(
                    &record.snapshot,
                    phase,
                    &format!("{} failed", error.operation()),
                    error.detail(),
                ) {
                    record.snapshot = snapshot;
                }
                format!(
                    "I couldn't finish this run: {}: {}. Send a message to correct, clarify, or continue it.",
                    error.operation(),
                    error.detail()
                )
            }
        };
        self.deliver_public_reply(record, public_reply);
        super::interaction::terminal_activity(record);
        let _ = persist_record(&self.inner.directory, record);
    }

    fn finish_nonaccepted(
        &self,
        record: &mut super::RunRecord,
        outcome: &ProductRunOutcome,
    ) -> String {
        let phase = match outcome.settlement().disposition() {
            RunDisposition::Cancelled => ProductRunPhase::Cancelled,
            RunDisposition::RecoveryRequired => ProductRunPhase::RecoveryRequired,
            RunDisposition::CandidateAvailable | RunDisposition::FailedNoCandidate => {
                ProductRunPhase::Failed
            }
            RunDisposition::Accepted | RunDisposition::WaitingForUser => return String::new(),
        };
        let has_candidate = outcome.candidate().is_some();
        let in_place = self.inner.folders.contains_key(&record.request.workspace_id());
        let status = match (outcome.settlement().disposition(), has_candidate) {
            (RunDisposition::Cancelled, _) if in_place => {
                "Cancelled — in-place effects retained, not verified complete"
            }
            (RunDisposition::RecoveryRequired, _) if in_place => {
                "Recovery required — in-place effects retained, not verified complete"
            }
            (_, _) if in_place => "Stopped — in-place effects retained, not verified complete",
            (RunDisposition::CandidateAvailable, _) => "Candidate available",
            (RunDisposition::RecoveryRequired, true) => "Recovery required — candidate preserved",
            (RunDisposition::RecoveryRequired, false) => "Recovery required",
            (RunDisposition::Cancelled, true) => "Cancelled — candidate preserved",
            (RunDisposition::Cancelled, false) => "Cancelled — no candidate",
            _ => "Stopped with no candidate",
        };
        let detail = outcome.detail().unwrap_or(status);
        let summary = terminal_summary(outcome, detail);
        if let Some(output) = outcome.candidate()
            && let Ok(snapshot) = ProductRunSnapshot::new(
                record.request.run_id(),
                record.request.workspace_id(),
                record.request.providers(),
                phase,
                output.fixer_cycles.saturating_add(1),
                record.request.task().to_owned(),
                status.to_owned(),
                output.diff.clone(),
                output.gates.clone(),
                output.review.clone(),
                summary.clone(),
            )
        {
            record.snapshot = self.with_candidate(record, outcome, snapshot);
        } else if let Ok(snapshot) = replace_snapshot(&record.snapshot, phase, status, &summary) {
            record.snapshot = snapshot;
        }
        let remaining = if outcome.remaining_work().is_empty() {
            String::new()
        } else {
            format!(" Remaining work: {}.", outcome.remaining_work().join("; "))
        };
        format!("{status}: {detail}.{remaining}")
    }

    fn deliver_public_reply(&self, record: &mut super::RunRecord, text: String) {
        if let Some(start) =
            record.interaction.as_ref().and_then(|options| options.workbench.as_ref())
            && self.with_controls(false, |store| store.publish_reply(start, &text)).is_err()
        {
            fail_handoff(record);
            if let Some(options) = &record.interaction {
                options.persistence_failed.store(true, std::sync::atomic::Ordering::Release);
            }
            return;
        }
        // Workbench history is authoritative in C0; legacy JSON keeps its bounded projection.
        let _ = record.conversation.append(ProductConversationRole::Agent, text);
    }

    fn with_candidate(
        &self,
        record: &super::RunRecord,
        outcome: &ProductRunOutcome,
        snapshot: ProductRunSnapshot,
    ) -> ProductRunSnapshot {
        match self.project_deliverable(record, outcome) {
            Some(deliverable) => snapshot.with_deliverable(deliverable),
            None => snapshot,
        }
    }

    fn project_deliverable(
        &self,
        record: &super::RunRecord,
        outcome: &ProductRunOutcome,
    ) -> Option<ProductDeliverable> {
        if self.inner.folders.contains_key(&record.request.workspace_id()) {
            return None;
        }
        let output = outcome.candidate()?;
        let stage = outcome.settlement().checkpoint()?.stage();
        let workspace = self.inner.workspaces.get(&record.request.workspace_id())?;
        ProductDeliverable::candidate(
            workspace.to_string_lossy().into_owned(),
            output.changed_paths.iter().map(|path| path.to_string_lossy().into_owned()).collect(),
            output.successful_commands.clone(),
            output.run_instructions.clone(),
            stage,
        )
        .ok()
    }
}

fn terminal_summary(outcome: &ProductRunOutcome, detail: &str) -> String {
    let mut summary = outcome
        .candidate()
        .map_or_else(|| detail.to_owned(), |candidate| candidate.summary.clone());
    if !detail.is_empty() && !summary.contains(detail) {
        summary.push_str("\n\nInterruption: ");
        summary.push_str(detail);
    }
    if !outcome.remaining_work().is_empty() && !summary.contains("Remaining work:") {
        summary.push_str("\n\nRemaining work:\n- ");
        summary.push_str(&outcome.remaining_work().join("\n- "));
    }
    summary
}

fn fail_handoff(record: &mut super::RunRecord) {
    let detail = "Passing checks could not be projected into a durable deliverable handoff";
    if let Ok(snapshot) = replace_snapshot(
        &record.snapshot,
        ProductRunPhase::Failed,
        "Create durable deliverable handoff failed",
        detail,
    ) {
        record.snapshot = snapshot;
    }
    let _ = record.conversation.append(ProductConversationRole::Agent, detail.to_owned());
}
