//! Durable terminal projection for one owned product-run attempt.

use peritus_app_protocol::{ProductDeliverable, ProductRunPhase, ProductRunSnapshot};
use peritus_product_runner::ProductRunOutcome;
use peritus_run_settlement::RunDisposition;
use peritus_types::{RunId, Sha256Digest};
use std::{
    sync::{Arc, atomic::AtomicBool},
    time::Duration,
};

use super::handoff::{fail_handoff, terminal_summary};
use crate::product_run::persistence::{
    ObligationFirstCause, ObligationRoot, SettlementObligation,
};
use crate::product_run::publication::{MutationDisposition, RunMutationKind};
use crate::product_run::snapshot::replace_snapshot;
use crate::product_run::{ProductRunService, ProductRunServiceError};

const TERMINAL_PERSIST_RETRY: Duration = Duration::from_millis(50);

enum TerminalProjection {
    Ready {
        binding: peritus_product_runner::control::ControlOperation,
        reply: String,
        expected_input_generation: u64,
        final_record: Option<super::super::RunRecord>,
    },
    Stop,
}

impl ProductRunService {
    pub(super) async fn finish_async(
        &self,
        run_id: RunId,
        attempt: Arc<AtomicBool>,
        result: Result<ProductRunOutcome, peritus_product_runner::ProductRunnerError>,
    ) -> Result<(), String> {
        let service = self.clone();
        let failure_attempt = Arc::clone(&attempt);
        match tokio::task::spawn_blocking(move || service.finish(run_id, &attempt, &result)).await {
            Ok(result) => {
                let detail = result.as_ref().err().cloned().unwrap_or_else(|| {
                    "the terminal path returned before proving its settlement obligation"
                        .to_owned()
                });
                self.finish_worker_failure(
                    run_id,
                    failure_attempt,
                    "settle product-run terminal owner",
                    detail,
                )
                .await;
                result
            }
            Err(error) => {
                let detail = error.to_string();
                self.finish_worker_failure(
                    run_id,
                    failure_attempt,
                    "publish product-run terminal state",
                    detail.clone(),
                )
                .await;
                Err(format!("product-run terminal publication failed: {detail}"))
            }
        }
    }

    fn finish(
        &self,
        run_id: RunId,
        attempt: &Arc<AtomicBool>,
        result: &Result<ProductRunOutcome, peritus_product_runner::ProductRunnerError>,
    ) -> Result<(), String> {
        let Some(terminal) = self.retain_terminal_outcome(run_id, attempt, result)? else {
            return Ok(());
        };
        if result.as_ref().is_ok_and(|outcome| {
            matches!(
                outcome.settlement().disposition(),
                RunDisposition::Accepted | RunDisposition::WaitingForUser
            )
        }) {
            // The runner has returned, so its in-place writes have reached a completed owned
            // boundary. A failed seal remains visibly unsealed and can never be overwritten.
            if let Err(error) =
                self.seal_latest_checkpoint(&terminal.interaction.workbench, run_id, attempt)
            {
                self.retain_terminal_boundary_failure(
                    run_id,
                    attempt,
                    result,
                    &error.describe(),
                    "Checkpoint sealing failed",
                    "The completed outcome is retained, but its latest workspace checkpoint could not be sealed. Explicit recovery is required before acceptance.",
                )?;
                return Ok(());
            }
        }
        let prepared_goal = match self.prepare_workbench_goal(&terminal, result) {
            Ok(prepared) => prepared,
            Err(error) => {
                self.retain_terminal_boundary_failure(
                    run_id,
                    attempt,
                    result,
                    &error.describe(),
                    "Goal settlement persistence failed",
                    "The runner stopped, but its terminal goal evidence could not be durably reconciled.",
                )?;
                return Ok(());
            }
        };
        let TerminalProjection::Ready {
            binding,
            reply: public_reply,
            expected_input_generation,
            final_record,
        } = self.retain_terminal_projection(run_id, attempt, result, None)?
        else {
            return Ok(());
        };
        let prepared_reply = self
            .prepare_public_reply(
                run_id,
                attempt,
                &binding,
                &public_reply,
                expected_input_generation,
            )
            .map_err(|error| error.describe())?;
        let reply_expected_revision = prepared_reply.expected_revision().unwrap_or(0);
        let obligation = self
            .retain_terminal_obligation(
                run_id,
                attempt,
                &binding,
                &public_reply,
                prepared_reply.reference(),
                reply_expected_revision,
                &prepared_goal,
                if self.user_cancellation_requested(run_id) {
                    ObligationFirstCause::UserCancellation
                } else if self.inner.control_shutdown.is_cancelled() {
                    ObligationFirstCause::DaemonShutdown
                } else if result.is_ok() {
                    ObligationFirstCause::RunnerOutcome
                } else {
                    ObligationFirstCause::RunnerFailure
                },
            )
            .map_err(|error| error.describe())?;
        let accepted = result.as_ref().is_ok_and(|outcome| {
            outcome.settlement().disposition() == RunDisposition::Accepted
        });
        let goal_status = self
            .commit_workbench_goal(
                prepared_goal,
                accepted,
                (expected_input_generation > 0).then_some(expected_input_generation),
            )
            .map_err(|error| error.describe())?;
        self.retain_goal_obligation_progress(run_id, attempt, obligation)
            .map_err(|error| error.describe())?;
        let published = self.publish_prepared_public_reply(
            run_id,
            attempt,
            prepared_reply,
            &public_reply,
            expected_input_generation,
        );
        self.retain_publication_result(
            run_id,
            attempt,
            published,
            final_record,
            obligation,
            goal_status,
        )?;
        // Improvement evidence must never outrun the authoritative terminal run record. Recovery
        // backfills a successfully persisted terminal record if the following collection is
        // interrupted.
        let retained = self
            .inner
            .records
            .read()
            .ok()
            .and_then(|records| records.get(&run_id).cloned())
            .filter(|record| Arc::ptr_eq(&record.cancelled, attempt));
        if let Some(record) = retained
            && let Err(error) = self.collect_improvement(&record)
        {
            let detail = error.describe();
            if let Ok(((), ticket)) = self.mutate_run(
                run_id,
                Some(attempt),
                RunMutationKind::ImprovementObservation,
                peritus_codec::sha256(detail.as_bytes()),
                MutationDisposition::DurabilityRequired,
                |record| {
                    record.interaction.append(
                        peritus_app_protocol::ProductActivityKind::Error,
                        "Could not collect an improvement suggestion. Open the inbox to retry collection.",
                        &detail,
                    )?;
                    Ok(())
                },
            ) {
                let _ = self.await_run_durable(ticket);
            }
        }
        Ok(())
    }

    pub(super) fn retain_terminal_obligation(
        &self,
        run: RunId,
        attempt: &Arc<AtomicBool>,
        start: &peritus_product_runner::control::ControlOperation,
        reply: &str,
        reply_reference: &peritus_product_runner::control::PublicReplyReference,
        reply_expected_revision: u64,
        goal: &super::goal::PreparedWorkbenchGoal,
        first_cause: ObligationFirstCause,
    ) -> Result<Sha256Digest, ProductRunServiceError> {
        let semantic = goal.binding();
        let reply_digest = peritus_codec::sha256(reply.as_bytes());
        let reply_bytes = u64::try_from(reply.len()).unwrap_or(u64::MAX);
        if reply_reference.digest() != reply_digest || reply_reference.bytes() != reply_bytes {
            return Err(ProductRunServiceError::InvalidState);
        }
        let frozen_unix_millis = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .ok()
            .and_then(|duration| u64::try_from(duration.as_millis()).ok())
            .unwrap_or(1)
            .max(1);
        let root = ObligationRoot::new(
            run,
            self.capture_run_identity(run)?.attempt_sequence,
            *start.id().as_bytes(),
            *start.conversation().as_bytes(),
            semantic.map_or([0; 16], |binding| binding.goal().into_bytes()),
            semantic.map_or(0, |binding| u64::from(binding.attempt())),
            semantic.map_or(0, crate::product_control::GoalSemanticBinding::user_revision),
            semantic.map(crate::product_control::GoalSemanticBinding::input_generation),
            goal.evidence_input_generation(),
            goal.settlement_tag(),
            goal.unresolved_effects(),
            frozen_unix_millis,
            reply_expected_revision,
            reply_reference.operation().into_bytes(),
            reply_reference.after_invocation().into_bytes(),
            reply.to_owned(),
            reply_digest,
            reply_bytes,
            first_cause,
        )?;
        let identity = root.identity();
        let (_, ticket) = self.mutate_run(
            run,
            Some(attempt),
            RunMutationKind::SettlementObligation,
            identity,
            MutationDisposition::DurabilityRequired,
            move |record| match record.settlement_obligation.as_ref() {
                Some(existing) if existing.identity() == identity => Ok(()),
                Some(_) => Err(ProductRunServiceError::InvalidState),
                None => {
                    record.settlement_obligation = Some(SettlementObligation::Pending(root));
                    Ok(())
                }
            },
        )?;
        self.await_run_durable(ticket)?;
        Ok(identity)
    }

    pub(super) fn retain_goal_obligation_progress(
        &self,
        run: RunId,
        attempt: &Arc<AtomicBool>,
        obligation: Sha256Digest,
    ) -> Result<(), ProductRunServiceError> {
        let (_, ticket) = self.mutate_run(
            run,
            Some(attempt),
            RunMutationKind::SettlementProgress,
            obligation,
            MutationDisposition::DurabilityRequired,
            move |record| {
                record
                    .settlement_obligation
                    .as_mut()
                    .ok_or(ProductRunServiceError::InvalidState)?
                    .mark_goal_committed(obligation)
            },
        )?;
        self.await_run_durable(ticket)
    }

    pub(in crate::product_run) fn recover_terminal_obligations(
        &self,
    ) -> Result<(), ProductRunServiceError> {
        let retained = self
            .inner
            .records
            .read()
            .map_err(|_| ProductRunServiceError::Unavailable)?
            .values()
            .filter_map(|record| {
                record.settlement_obligation.as_ref().map(|obligation| {
                    (
                        record.request.run_id(),
                        record.clone(),
                        obligation.clone(),
                    )
                })
            })
            .collect::<Vec<_>>();
        for (run, record, obligation) in retained {
            match obligation {
                SettlementObligation::Resolved(_)
                | SettlementObligation::ResolvedWithLegacyGap(_) => continue,
                SettlementObligation::LegacyAssessmentRequired(_) => {
                    let identity = obligation.identity();
                    let (_, ticket) = self.mutate_run(
                        run,
                        Some(&record.cancelled),
                        RunMutationKind::Recovery,
                        identity,
                        MutationDisposition::DurabilityRequired,
                        move |record| {
                            record
                                .settlement_obligation
                                .as_mut()
                                .ok_or(ProductRunServiceError::InvalidState)?
                                .resolve_legacy_gap(identity)
                        },
                    )?;
                    self.await_run_durable(ticket)?;
                }
                SettlementObligation::Pending(root) => {
                    if root.run != run
                        || root.attempt != record.attempt_sequence
                        || root.start_operation != *record.interaction.workbench.id().as_bytes()
                        || root.conversation
                            != *record.interaction.workbench.conversation().as_bytes()
                    {
                        return Err(ProductRunServiceError::InvalidMessage);
                    }
                    let mut goal_status = None;
                    if root.goal_pending() {
                        let prepared = self.prepare_bound_workbench_goal(
                            &record,
                            root.settlement,
                            root.evidence_input_generation,
                            root.unresolved_effects,
                        )?;
                        if !recovered_goal_binding_matches(&root, prepared.binding()) {
                            return Err(ProductRunServiceError::InvalidState);
                        }
                        goal_status = self.commit_workbench_goal(
                            prepared,
                            root.settlement
                                == peritus_product_runner::control::GoalSettlement::Accepted
                                    as u16,
                            root.evidence_input_generation,
                        )?;
                        self.retain_goal_obligation_progress(
                            run,
                            &record.cancelled,
                            root.identity(),
                        )?;
                    }
                    if root.reply_pending() {
                        let prepared = self.prepare_recovery_public_reply(
                            run,
                            &record.cancelled,
                            &record.interaction.workbench,
                            &root.reply_text,
                            root.evidence_input_generation.unwrap_or(0),
                            &root.reply_reference()?,
                            root.reply_expected_revision,
                        )?;
                        if prepared.reference() != &root.reply_reference()? {
                            return Err(ProductRunServiceError::InvalidState);
                        }
                        let published = self.publish_prepared_public_reply(
                            run,
                            &record.cancelled,
                            prepared,
                            &root.reply_text,
                            root.evidence_input_generation.unwrap_or(0),
                        );
                        self.retain_publication_result(
                            run,
                            &record.cancelled,
                            published,
                            None,
                            root.identity(),
                            goal_status,
                        )
                        .map_err(|detail| {
                            ProductRunServiceError::internal(
                                "recover terminal obligation",
                                detail,
                            )
                        })?;
                        if !published {
                            return Err(ProductRunServiceError::InvalidState);
                        }
                    }
                    self.retire_run_cancellation(run, &record.cancelled);
                }
            }
        }
        Ok(())
    }

    fn retain_terminal_outcome(
        &self,
        run_id: RunId,
        attempt: &Arc<AtomicBool>,
        result: &Result<ProductRunOutcome, peritus_product_runner::ProductRunnerError>,
    ) -> Result<Option<super::super::RunRecord>, String> {
        let rejected_boundary = self
            .inner
            .records
            .read()
            .ok()
            .and_then(|records| records.get(&run_id).cloned())
            .is_some_and(|record| {
                Arc::ptr_eq(&record.cancelled, attempt)
                    && record.rejected_finding_update.is_some()
                    && matches!(
                        record.snapshot.phase(),
                        ProductRunPhase::RecoveryRequired | ProductRunPhase::Cancelled
                    )
            });
        if rejected_boundary {
            self.retain_rejected_terminal_handoff(run_id, attempt, result)
                .map_err(|error| error.describe())?;
            return Ok(None);
        }
        let mut reported = false;
        loop {
            match self.try_retain_terminal_outcome(run_id, attempt, result) {
                Ok(retained) => return Ok(retained),
                Err(error) => {
                    if !reported {
                        crate::diagnostic::report(&format!(
                            "peritusd: terminal outcome persistence is retrying under its retained owner: {}",
                            error.describe(),
                        ));
                        reported = true;
                    }
                    if self.inner.control_shutdown.is_cancelled()
                        || self.inner.control_reconciliation.is_cancelled()
                    {
                        return Err(format!(
                            "daemon shutdown interrupted retained terminal outcome persistence: {}",
                            error.describe(),
                        ));
                    }
                    std::thread::sleep(TERMINAL_PERSIST_RETRY);
                }
            }
        }
    }

    /// Enriches a rejected observer boundary without overwriting its immutable handoff sequence.
    /// All continuation publication and filesystem retry happens after releasing the registry.
    fn retain_rejected_terminal_handoff(
        &self,
        run_id: RunId,
        attempt: &Arc<AtomicBool>,
        result: &Result<ProductRunOutcome, peritus_product_runner::ProductRunnerError>,
    ) -> Result<(), ProductRunServiceError> {
        let user_cancelled = self.user_cancellation_requested(run_id);
        let cancellation_generation = user_cancelled
            .then(|| self.pending_user_cancellation_generation(run_id, attempt))
            .flatten();
        let (prepared, ticket) = self.mutate_run(
            run_id,
            Some(attempt),
            RunMutationKind::ExecutionObservation,
            terminal_result_digest(result),
            MutationDisposition::DurabilityRequired,
            |record| {
                if record.rejected_finding_update.is_none()
                    || !matches!(
                        record.snapshot.phase(),
                        ProductRunPhase::RecoveryRequired | ProductRunPhase::Cancelled
                    )
                {
                    return Ok(None);
                }
                let prior_sequence = record.handoff_sequence;
                let attempt_sequence = record.attempt_sequence;
                let mut next = record.clone();
                next.user_cancelled |= user_cancelled;
                if let Some(source) = next
                    .continuation_sources
                    .iter_mut()
                    .rev()
                    .find(|source| !source.settled)
                {
                    source.settled = true;
                }
                match result {
                    Ok(outcome) => {
                        next.checkpoint = outcome.settlement().checkpoint().copied();
                        next.settlement = Some(*outcome.settlement());
                        next.resume = outcome.resume().cloned();
                        next.remaining_work = outcome.remaining_work().to_vec();
                        next.interruption_cause =
                            outcome.detail().map_or_else(String::new, str::to_owned);
                        next.candidate_actionable =
                            !self.inner.folders.contains_key(&next.request.workspace_id())
                                && outcome.candidate().is_some()
                                && outcome.settlement().checkpoint().is_some();
                        if let Some(output) = outcome.candidate()
                            && let Ok(operation) = super::super::operation::retained_execution(
                                run_id,
                                ProductRunPhase::RecoveryRequired,
                                &next.interruption_cause,
                            )
                            && let Ok(snapshot) = ProductRunSnapshot::new(
                                run_id,
                                next.request.workspace_id(),
                                next.request.providers(),
                                ProductRunPhase::RecoveryRequired,
                                output.fixer_cycles.saturating_add(1),
                                next.request.display_task().to_owned(),
                                "Terminal outcome retained after rejected finding update"
                                    .to_owned(),
                                output.diff.clone(),
                                output.gates.clone(),
                                output.review.clone(),
                                output.summary.clone(),
                                operation,
                            )
                        {
                            next.snapshot = self.with_candidate(&next, outcome, snapshot);
                        }
                    }
                    Err(error) => {
                        next.interruption_cause =
                            format!("{}: {}", error.operation(), error.detail());
                        next.candidate_actionable = false;
                    }
                }
                if next.user_cancelled {
                    next.snapshot = replace_snapshot(
                        &next.snapshot,
                        ProductRunPhase::Cancelled,
                        "Run cancelled",
                        "Cancelled while rejected finding evidence was retained",
                    )?;
                }
                next.handoff_sequence = prior_sequence.checked_add(1).ok_or_else(|| {
                    ProductRunServiceError::internal(
                        "publish terminal product-run handoff",
                        "handoff sequence overflow",
                    )
                })?;
                next.handoff_recovery_pending = true;
                record.checkpoint = next.checkpoint;
                record.settlement = next.settlement;
                record.remaining_work.clone_from(&next.remaining_work);
                record.interruption_cause.clone_from(&next.interruption_cause);
                record.candidate_actionable = next.candidate_actionable;
                record.user_cancelled = next.user_cancelled;
                if next.user_cancelled {
                    record.snapshot = next.snapshot.clone();
                }
                record.handoff_recovery_pending = true;
                Ok(Some((next, prior_sequence, attempt_sequence)))
            },
        )?;
        self.await_run_durable(ticket)?;
        let Some((mut handoff, prior_sequence, attempt_sequence)) = prepared else {
            return Ok(());
        };

        self.externalize_terminal_review_artifacts(&mut handoff)?;
        let handoff_cancellation = if self.inner.control_shutdown.is_cancelled() {
            &self.inner.control_shutdown
        } else {
            &self.inner.control_reconciliation
        };
        super::super::persistence::write_handoff_retrying(
            &self.inner.directory,
            &handoff,
            super::super::persistence::HandoffKind::Terminal,
            handoff_cancellation,
        )?;
        handoff.handoff_recovery_pending = false;
        let ((installed, cancellation_handoff), ticket) = self.mutate_run(
            run_id,
            Some(attempt),
            RunMutationKind::Recovery,
            terminal_result_digest(result),
            MutationDisposition::DurabilityRequired,
            move |record| {
                if record.attempt_sequence != attempt_sequence
                    || record.handoff_sequence != prior_sequence
                    || record.rejected_finding_update.is_none()
                {
                    return Ok((false, None));
                }
                let cancellation_added = record.user_cancelled && !handoff.user_cancelled;
                handoff.user_cancelled |= record.user_cancelled;
                if handoff.user_cancelled {
                    handoff.snapshot = replace_snapshot(
                        &handoff.snapshot,
                        ProductRunPhase::Cancelled,
                        "Run cancelled",
                        "Cancelled while terminal finding evidence was retained",
                    )
                    .unwrap_or_else(|_| record.snapshot.clone());
                }
                handoff.interaction = record.interaction.clone();
                handoff.preview = record.preview.clone();
                install_terminal_handoff_record(record, handoff);
                let cancellation = if cancellation_added {
                    let prior_sequence = record.handoff_sequence;
                    let mut cancellation = record.clone();
                    cancellation.handoff_sequence =
                        prior_sequence.checked_add(1).ok_or_else(|| {
                            ProductRunServiceError::internal(
                                "publish terminal cancellation handoff",
                                "handoff sequence overflow",
                            )
                        })?;
                    cancellation.snapshot = replace_snapshot(
                        &cancellation.snapshot,
                        ProductRunPhase::Cancelled,
                        "Run cancelled",
                        "Cancelled while terminal finding evidence was retained",
                    )?;
                    cancellation.handoff_recovery_pending = true;
                    record.handoff_recovery_pending = true;
                    Some((cancellation, prior_sequence, record.attempt_sequence))
                } else {
                    None
                };
                Ok((true, cancellation))
            },
        )?;
        self.await_run_durable(ticket)?;
        if installed
            && let Some(generation) = cancellation_generation
        {
            self.acknowledge_user_cancellation(run_id, attempt, generation)?;
        }
        let cancellation_generation = self.pending_user_cancellation_generation(run_id, attempt);
        let Some((
            mut cancellation,
            prior_sequence,
            attempt_sequence,
        )) = cancellation_handoff
        else {
            return Ok(());
        };
        let handoff_cancellation = if self.inner.control_shutdown.is_cancelled() {
            &self.inner.control_shutdown
        } else {
            &self.inner.control_reconciliation
        };
        super::super::persistence::write_handoff_retrying(
            &self.inner.directory,
            &cancellation,
            super::super::persistence::HandoffKind::Terminal,
            handoff_cancellation,
        )?;
        cancellation.handoff_recovery_pending = false;
        let (installed, ticket) = self.mutate_run(
            run_id,
            Some(attempt),
            RunMutationKind::Recovery,
            peritus_codec::sha256(b"install-terminal-cancellation-handoff"),
            MutationDisposition::DurabilityRequired,
            move |record| {
                if record.attempt_sequence != attempt_sequence
                    || record.handoff_sequence != prior_sequence
                    || !record.user_cancelled
                {
                    return Ok(false);
                }
                cancellation.interaction = record.interaction.clone();
                cancellation.preview = record.preview.clone();
                install_terminal_handoff_record(record, cancellation);
                Ok(true)
            },
        )?;
        self.await_run_durable(ticket)?;
        if installed
            && let Some(generation) = cancellation_generation
        {
            self.acknowledge_user_cancellation(run_id, attempt, generation)?;
        }
        Ok(())
    }

    fn externalize_terminal_review_artifacts(
        &self,
        record: &mut super::super::RunRecord,
    ) -> Result<(), ProductRunServiceError> {
        let mut reported = false;
        loop {
            match self.inner.finding_bodies.migrate_record(record) {
                Ok(_) => return Ok(()),
                Err(error) => {
                    if !reported {
                        crate::diagnostic::report(&format!(
                            "peritusd: terminal review-artifact publication is retrying under its retained owner: {}",
                            error.describe(),
                        ));
                        reported = true;
                    }
                    if self.inner.control_shutdown.is_cancelled()
                        || self.inner.control_reconciliation.is_cancelled()
                    {
                        return Err(error);
                    }
                    std::thread::sleep(TERMINAL_PERSIST_RETRY);
                }
            }
        }
    }

    fn try_retain_terminal_outcome(
        &self,
        run_id: RunId,
        attempt: &Arc<AtomicBool>,
        result: &Result<ProductRunOutcome, peritus_product_runner::ProductRunnerError>,
    ) -> Result<Option<super::super::RunRecord>, ProductRunServiceError> {
        let prepared_deliverable = result
            .as_ref()
            .ok()
            .map(|outcome| self.prepare_deliverable(run_id, outcome))
            .transpose()?
            .flatten();
        let input = terminal_result_digest(result);
        let (retained, ticket) = self.mutate_run(
            run_id,
            Some(attempt),
            RunMutationKind::ExecutionObservation,
            input,
            MutationDisposition::DurabilityRequired,
            |record| {
        if self.user_cancellation_requested(run_id) {
            record.user_cancelled = true;
        }
        if let Some(source) = record.continuation_sources.iter_mut().rev().find(|source| !source.settled)
        {
            source.settled = true;
        }
        if let Ok(outcome) = result {
            record.checkpoint = outcome.settlement().checkpoint().copied();
            record.settlement = Some(*outcome.settlement());
            record.resume = outcome.resume().cloned();
            record.remaining_work = outcome.remaining_work().to_vec();
            record.interruption_cause = outcome.detail().map_or_else(String::new, str::to_owned);
            record.candidate_actionable =
                !self.inner.folders.contains_key(&record.request.workspace_id())
                    && outcome.candidate().is_some()
                    && outcome.settlement().checkpoint().is_some();
        }
        let projection_retained = match result {
            Ok(outcome)
                if outcome.settlement().disposition() == RunDisposition::Accepted =>
            {
                self.retain_pending_accepted_projection(
                    record,
                    outcome,
                    prepared_deliverable.clone(),
                )
            }
            _ => true,
        };
        // Project the completed output before capturing its preimage so a baseline failure is the
        // last authoritative status and cannot be obscured by the pending-publication snapshot.
        let baseline_retained = self.retain_task_baseline(record);
        Ok((baseline_retained && projection_retained).then(|| record.clone()))
            },
        )?;
        self.await_run_durable(ticket)?;
        Ok(retained)
    }

    fn retain_pending_accepted_projection(
        &self,
        record: &mut super::super::RunRecord,
        outcome: &ProductRunOutcome,
        deliverable: Option<ProductDeliverable>,
    ) -> bool {
        let Some(output) = outcome.candidate() else {
            retain_pending_projection_failure(
                record,
                "accepted settlement omitted its completed candidate",
            );
            return false;
        };
        if deliverable.is_none()
            && !self.inner.folders.contains_key(&record.request.workspace_id())
        {
            retain_pending_projection_failure(
                record,
                "completed managed candidate could not be projected as a durable deliverable",
            );
            return false;
        }
        let operation = match super::super::operation::retained_execution(
            record.request.run_id(),
            ProductRunPhase::RecoveryRequired,
            &record.interruption_cause,
        ) {
            Ok(operation) => operation,
            Err(_) => {
                retain_pending_projection_failure(
                    record,
                    "completed candidate operation identity could not be retained",
                );
                return false;
            }
        };
        let snapshot = match ProductRunSnapshot::new(
            record.request.run_id(),
            record.request.workspace_id(),
            record.request.providers(),
            ProductRunPhase::RecoveryRequired,
            output.fixer_cycles.saturating_add(1),
            record.request.display_task().to_owned(),
            "Completed outcome retained — terminal publication pending".to_owned(),
            output.diff.clone(),
            output.gates.clone(),
            output.review.clone(),
            output.summary.clone(),
            operation,
        ) {
            Ok(snapshot) => snapshot,
            Err(_) => {
                retain_pending_projection_failure(
                    record,
                    "completed candidate snapshot could not be retained",
                );
                return false;
            }
        };
        let snapshot = match deliverable {
            Some(deliverable) => snapshot.with_deliverable(deliverable),
            None => snapshot,
        };
        if self
            .inner
            .product_artifacts
            .publish_snapshot(record.request.run_id(), &snapshot)
            .is_err()
        {
            retain_pending_projection_failure(
                record,
                "completed candidate artifacts could not be prepared for exact retrieval",
            );
            return false;
        }
        record.snapshot = snapshot;
        true
    }

    fn retain_terminal_boundary_failure(
        &self,
        run_id: RunId,
        attempt: &Arc<AtomicBool>,
        result: &Result<ProductRunOutcome, peritus_product_runner::ProductRunnerError>,
        detail: &str,
        status: &'static str,
        summary: &'static str,
    ) -> Result<(), String> {
        let mut reported = false;
        loop {
            match self.try_retain_terminal_boundary_failure(
                run_id,
                attempt,
                result,
                detail,
                status,
                summary,
            ) {
                Ok(()) => return Ok(()),
                Err(error) => {
                    if !reported {
                        crate::diagnostic::report(&format!(
                            "peritusd: terminal-boundary failure persistence is retrying under its retained owner: {}",
                            error.describe(),
                        ));
                        reported = true;
                    }
                    if self.inner.control_shutdown.is_cancelled()
                        || self.inner.control_reconciliation.is_cancelled()
                    {
                        return Err(format!(
                            "daemon shutdown interrupted terminal-boundary failure persistence: {}",
                            error.describe(),
                        ));
                    }
                    std::thread::sleep(TERMINAL_PERSIST_RETRY);
                }
            }
        }
    }

    fn try_retain_terminal_boundary_failure(
        &self,
        run_id: RunId,
        attempt: &Arc<AtomicBool>,
        result: &Result<ProductRunOutcome, peritus_product_runner::ProductRunnerError>,
        detail: &str,
        status: &'static str,
        summary: &'static str,
    ) -> Result<(), ProductRunServiceError> {
        let ((), ticket) = self.mutate_run(
            run_id,
            Some(attempt),
            RunMutationKind::ExecutionFailure,
            peritus_codec::sha256(detail.as_bytes()),
            MutationDisposition::DurabilityRequired,
            |record| {
        record.interaction.record_persistence_failure(detail.to_owned());
        let accepted = result.as_ref().is_ok_and(|outcome| {
            outcome.settlement().disposition() == RunDisposition::Accepted
        });
        let user_cancelled = record.user_cancelled && !accepted;
        if let Ok(snapshot) = replace_snapshot(
            &record.snapshot,
            if user_cancelled {
                ProductRunPhase::Cancelled
            } else {
                ProductRunPhase::RecoveryRequired
            },
            if user_cancelled {
                "Run cancelled"
            } else {
                status
            },
            if user_cancelled {
                "Cancelled before terminal goal settlement could be reconciled."
            } else {
                summary
            },
        ) {
            record.snapshot = snapshot;
        }
        Ok(())
            },
        )?;
        self.await_run_durable(ticket)
    }

    fn retain_terminal_projection(
        &self,
        run_id: RunId,
        attempt: &Arc<AtomicBool>,
        result: &Result<ProductRunOutcome, peritus_product_runner::ProductRunnerError>,
        goal_status: Option<&str>,
    ) -> Result<TerminalProjection, String> {
        let mut reported = false;
        loop {
            match self.try_retain_terminal_projection(run_id, attempt, result, goal_status) {
                Ok(projection) => return Ok(projection),
                Err(error) => {
                    if !reported {
                        crate::diagnostic::report(&format!(
                            "peritusd: terminal projection persistence is retrying under its retained owner: {}",
                            error.describe(),
                        ));
                        reported = true;
                    }
                    if self.inner.control_shutdown.is_cancelled()
                        || self.inner.control_reconciliation.is_cancelled()
                    {
                        return Err(format!(
                            "daemon shutdown interrupted retained terminal projection persistence: {}",
                            error.describe(),
                        ));
                    }
                    std::thread::sleep(TERMINAL_PERSIST_RETRY);
                }
            }
        }
    }

    #[allow(clippy::too_many_lines, reason = "terminal projection order stays explicit")]
    fn try_retain_terminal_projection(
        &self,
        run_id: RunId,
        attempt: &Arc<AtomicBool>,
        result: &Result<ProductRunOutcome, peritus_product_runner::ProductRunnerError>,
        goal_status: Option<&str>,
    ) -> Result<TerminalProjection, ProductRunServiceError> {
        let (projection, ticket) = self.mutate_run(
            run_id,
            Some(attempt),
            RunMutationKind::ExecutionObservation,
            terminal_result_digest(result),
            MutationDisposition::DurabilityRequired,
            |record| {
        let mut next = record.clone();
        let accepted = result.as_ref().is_ok_and(|outcome| {
            outcome.settlement().disposition() == RunDisposition::Accepted
        });
        let user_cancelled = next.user_cancelled && !accepted;
        let shutdown_interrupted =
            self.inner.control_shutdown.is_cancelled() && !next.user_cancelled && !accepted;
        let public_reply = match result {
            _ if user_cancelled => {
                if let Ok(snapshot) = replace_snapshot(
                    &next.snapshot,
                    ProductRunPhase::Cancelled,
                    "Run cancelled",
                    "Cancelled before terminal settlement completed",
                ) {
                    next.snapshot = snapshot;
                }
                "Cancelled before terminal settlement completed".to_owned()
            }
            _ if shutdown_interrupted => {
                if let Ok(snapshot) = replace_snapshot(
                    &next.snapshot,
                    ProductRunPhase::RecoveryRequired,
                    "Daemon shutdown interrupted this run; explicit retry is required after restart",
                    next.snapshot.summary(),
                ) {
                    next.snapshot = snapshot;
                }
                "Daemon shutdown interrupted this run; explicit retry is required after restart"
                    .to_owned()
            }
            Ok(outcome) if outcome.settlement().disposition() == RunDisposition::Accepted => {
                let Some(output) = outcome.candidate() else {
                    fail_handoff(&mut next);
                    *record = next;
                    return Ok(TerminalProjection::Stop);
                };
                let completion_message = format!("Completed: {}", output.summary);
                let deliverable = next.snapshot.deliverable().cloned();
                if deliverable.is_none()
                    && !self.inner.folders.contains_key(&next.request.workspace_id())
                {
                    fail_handoff(&mut next);
                    *record = next;
                    return Ok(TerminalProjection::Stop);
                }
                if let Ok(operation) = super::super::operation::retained_execution(
                    run_id,
                    ProductRunPhase::Complete,
                    &next.interruption_cause,
                ) && let Ok(snapshot) = ProductRunSnapshot::new(
                    run_id,
                    next.request.workspace_id(),
                    next.request.providers(),
                    ProductRunPhase::Complete,
                    output.fixer_cycles + 1,
                    next.request.display_task().to_owned(),
                    if deliverable.is_some() {
                        "Qualified — passing checks and independent review".to_owned()
                    } else {
                        "Completed in place — tracked task files passed checks and independent review".to_owned()
                    },
                    output.diff.clone(),
                    output.gates.clone(),
                    output.review.clone(),
                    output.summary.clone(),
                    operation,
                ) {
                    let snapshot = match deliverable {
                        Some(deliverable) => snapshot.with_deliverable(deliverable),
                        None => snapshot,
                    };
                    self.inner.product_artifacts.publish_snapshot(run_id, &snapshot)?;
                    next.snapshot = snapshot;
                } else {
                    fail_handoff(&mut next);
                    *record = next;
                    return Ok(TerminalProjection::Stop);
                }
                completion_message
            }
            Ok(outcome) if outcome.settlement().disposition() == RunDisposition::WaitingForUser => {
                let Some(question) = outcome.question() else {
                    fail_handoff(&mut next);
                    *record = next;
                    return Ok(TerminalProjection::Stop);
                };
                let chatting =
                    next.interaction.mode != peritus_app_protocol::ProductInteractionMode::Build;
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
                    &next.snapshot,
                    ProductRunPhase::WaitingForUser,
                    status,
                    &terminal_summary(outcome, question.message()),
                ) {
                    next.snapshot = self.with_candidate(&next, outcome, snapshot);
                }
                question.message().to_owned()
            }
            Ok(outcome) => self.finish_nonaccepted(&mut next, outcome),
            Err(error) => {
                let phase =
                    if error.kind() == peritus_product_runner::ProductRunnerErrorKind::Cancelled {
                        ProductRunPhase::Cancelled
                    } else {
                        ProductRunPhase::Failed
                    };
                if let Ok(snapshot) = replace_snapshot(
                    &next.snapshot,
                    phase,
                    &format!("{} failed", error.operation()),
                    error.detail(),
                ) {
                    next.snapshot = snapshot;
                }
                format!(
                    "I couldn't finish this run: {}: {}. Send a message to correct, clarify, or continue it.",
                    error.operation(),
                    error.detail()
                )
            }
        };
        if !user_cancelled
            && !shutdown_interrupted
            && let Some(status) = goal_status
            && let Ok(snapshot) = replace_snapshot(
                &next.snapshot,
                next.snapshot.phase(),
                status,
                next.snapshot.summary(),
            )
        {
            next.snapshot = snapshot;
        }
        let final_record = accepted.then(|| next.clone());
        if !accepted {
            *record = next.clone();
        }
        let binding = next.interaction.workbench.clone();
        Ok(TerminalProjection::Ready {
            binding,
            reply: public_reply,
            expected_input_generation: next.interaction.incorporated,
            final_record,
        })
            },
        )?;
        self.await_run_durable(ticket)?;
        Ok(projection)
    }

    pub(super) fn retain_publication_result(
        &self,
        run_id: RunId,
        attempt: &Arc<AtomicBool>,
        published: bool,
        final_record: Option<super::super::RunRecord>,
        obligation: Sha256Digest,
        goal_status: Option<&'static str>,
    ) -> Result<(), String> {
        let mut reported = false;
        loop {
            match self.try_retain_publication_result(
                run_id,
                attempt,
                published,
                final_record.as_ref(),
                obligation,
                goal_status,
            ) {
                Ok(()) => return Ok(()),
                Err(error) => {
                    if !reported {
                        crate::diagnostic::report(&format!(
                            "peritusd: public-reply reconciliation is retrying under its retained owner: {}",
                            error.describe(),
                        ));
                        reported = true;
                    }
                    if self.inner.control_shutdown.is_cancelled()
                        || self.inner.control_reconciliation.is_cancelled()
                    {
                        return Err(format!(
                            "daemon shutdown interrupted public-reply reconciliation: {}",
                            error.describe(),
                        ));
                    }
                    std::thread::sleep(TERMINAL_PERSIST_RETRY);
                }
            }
        }
    }

    fn try_retain_publication_result(
        &self,
        run_id: RunId,
        attempt: &Arc<AtomicBool>,
        published: bool,
        final_record: Option<&super::super::RunRecord>,
        obligation: Sha256Digest,
        goal_status: Option<&'static str>,
    ) -> Result<(), ProductRunServiceError> {
        let final_record = final_record.cloned();
        let (_, ticket) = self.mutate_run(
            run_id,
            Some(attempt),
            RunMutationKind::SettlementProgress,
            obligation,
            MutationDisposition::DurabilityRequired,
            move |record| {
                if published {
                    let recover_accepted_projection = final_record.is_none()
                        && record
                            .settlement_obligation
                            .as_ref()
                            .is_some_and(SettlementObligation::accepted_settlement)
                        && record.snapshot.phase() == ProductRunPhase::RecoveryRequired;
                    if let Some(final_record) = final_record {
                        let record_revision = record.record_revision;
                        let record_lineage_root = record.record_lineage_root;
                        let durable_record_revision = record.durable_record_revision;
                        let durable_lineage_root = record.durable_lineage_root;
                        let durable_canonical_digest = record.durable_canonical_digest;
                        let settlement_obligation = record.settlement_obligation.clone();
                        *record = final_record;
                        record.record_revision = record_revision;
                        record.record_lineage_root = record_lineage_root;
                        record.durable_record_revision = durable_record_revision;
                        record.durable_lineage_root = durable_lineage_root;
                        record.durable_canonical_digest = durable_canonical_digest;
                        record.settlement_obligation = settlement_obligation;
                    }
                    record
                        .settlement_obligation
                        .as_mut()
                        .ok_or(ProductRunServiceError::InvalidState)?
                        .mark_reply_committed(obligation)?;
                    if recover_accepted_projection {
                        let status = if record.snapshot.deliverable().is_some() {
                            "Qualified — passing checks and independent review"
                        } else {
                            "Completed in place — tracked task files passed checks and independent review"
                        };
                        record.snapshot = replace_snapshot(
                            &record.snapshot,
                            ProductRunPhase::Complete,
                            status,
                            record.snapshot.summary(),
                        )?;
                    }
                } else {
                    if !matches!(
                        record.snapshot.phase(),
                        ProductRunPhase::Cancelled | ProductRunPhase::RecoveryRequired
                    ) {
                        fail_handoff(record);
                    }
                }
                if let Some(status) = goal_status
                    && let Ok(snapshot) = replace_snapshot(
                        &record.snapshot,
                        record.snapshot.phase(),
                        status,
                        record.snapshot.summary(),
                    )
                {
                    record.snapshot = snapshot;
                }
                super::super::interaction::terminal_activity(record);
                Ok(())
            },
        )?;
        self.await_run_durable(ticket)
    }

    fn finish_nonaccepted(
        &self,
        record: &mut super::super::RunRecord,
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
            && let Ok(operation) = super::super::operation::retained_execution(
                record.request.run_id(),
                phase,
                &record.interruption_cause,
            )
            && let Ok(snapshot) = ProductRunSnapshot::new(
                record.request.run_id(),
                record.request.workspace_id(),
                record.request.providers(),
                phase,
                output.fixer_cycles.saturating_add(1),
                record.request.display_task().to_owned(),
                status.to_owned(),
                output.diff.clone(),
                output.gates.clone(),
                output.review.clone(),
                summary.clone(),
                operation,
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

    pub(super) fn publish_public_reply(
        &self,
        run: peritus_types::RunId,
        attempt: &Arc<AtomicBool>,
        binding: &peritus_product_runner::control::ControlOperation,
        text: &str,
        expected_input_generation: u64,
    ) -> bool {
        let prepared = match self.prepare_public_reply(
            run,
            attempt,
            binding,
            text,
            expected_input_generation,
        ) {
            Ok(prepared) => prepared,
            Err(_) => return false,
        };
        self.publish_prepared_public_reply(
            run,
            attempt,
            prepared,
            text,
            expected_input_generation,
        )
    }

    pub(super) fn prepare_public_reply(
        &self,
        run: peritus_types::RunId,
        attempt: &Arc<AtomicBool>,
        binding: &peritus_product_runner::control::ControlOperation,
        text: &str,
        expected_input_generation: u64,
    ) -> Result<crate::product_control::PreparedPublicReply, ProductRunServiceError> {
        let digest = peritus_codec::sha256(text.as_bytes());
        let reconciliation = match self.retained_control_reconciliation(run, attempt) {
            Ok(reconciliation) => reconciliation,
            Err(error) => return Err(error),
        };
        self.with_control_reconciliation(&reconciliation, |store| {
            if expected_input_generation == 0 {
                store.prepare_public_reply(binding, digest, text.len() as u64)
            } else {
                store.prepare_public_reply_for_generation(
                    binding,
                    digest,
                    text.len() as u64,
                    expected_input_generation,
                )
            }
        })
        .map_err(Into::into)
    }

    fn prepare_recovery_public_reply(
        &self,
        run: peritus_types::RunId,
        attempt: &Arc<AtomicBool>,
        binding: &peritus_product_runner::control::ControlOperation,
        text: &str,
        expected_input_generation: u64,
        reference: &peritus_product_runner::control::PublicReplyReference,
        expected_revision: u64,
    ) -> Result<crate::product_control::PreparedPublicReply, ProductRunServiceError> {
        if expected_revision == 0 {
            return self.prepare_public_reply(
                run,
                attempt,
                binding,
                text,
                expected_input_generation,
            );
        }
        if reference.digest() != peritus_codec::sha256(text.as_bytes())
            || reference.bytes() != u64::try_from(text.len()).unwrap_or(u64::MAX)
        {
            return Err(ProductRunServiceError::InvalidState);
        }
        let reconciliation = self.retained_control_reconciliation(run, attempt)?;
        self.with_control_reconciliation(&reconciliation, |store| {
            store.prepare_public_reply_recovery(binding, reference, expected_revision)
        })
        .map_err(Into::into)
    }

    pub(super) fn publish_prepared_public_reply(
        &self,
        run: peritus_types::RunId,
        attempt: &Arc<AtomicBool>,
        prepared: crate::product_control::PreparedPublicReply,
        text: &str,
        expected_input_generation: u64,
    ) -> bool {
        if prepared.retained() {
            return self.ensure_public_reply_artifact(prepared.reference()).is_ok();
        }
        let operation = prepared.reference().operation();
        let published = match prepared.publish(text) {
            Ok(published) => published,
            Err(_) => return false,
        };
        let reconciliation = match self.retained_control_reconciliation(run, attempt) {
            Ok(reconciliation) => reconciliation,
            Err(_) => return false,
        };
        let accepted = self.with_control_reconciliation(&reconciliation, |store| {
            if expected_input_generation == 0 {
                store.accept_published_reply(published)
            } else {
                store.accept_published_reply_for_generation(
                    published,
                    expected_input_generation,
                )
            }
        });
        match accepted {
            Ok(_) => self
                .inner
                .retained_reply_owners
                .lock()
                .map(|mut retained| retained.insert(operation))
                .is_ok(),
            // Retain the durable marker and pin. Restart reconciliation removes it only after C0
            // proves the operation absent; an ambiguous commit can never lose its body.
            Err(_) => false,
        }
    }

    fn with_candidate(
        &self,
        record: &super::super::RunRecord,
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
        record: &super::super::RunRecord,
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

    fn prepare_deliverable(
        &self,
        run: RunId,
        outcome: &ProductRunOutcome,
    ) -> Result<Option<ProductDeliverable>, ProductRunServiceError> {
        let record = self
            .inner
            .records
            .read()
            .map_err(|_| ProductRunServiceError::Unavailable)?
            .get(&run)
            .cloned()
            .ok_or(ProductRunServiceError::NotFound)?;
        let deliverable = self.project_deliverable(&record, outcome);
        if let Some(deliverable) = &deliverable {
            self.inner.product_artifacts.publish_deliverable(run, deliverable)?;
        }
        Ok(deliverable)
    }
}

fn install_terminal_handoff_record(
    record: &mut super::super::RunRecord,
    replacement: super::super::RunRecord,
) {
    let record_revision = record.record_revision;
    let record_lineage_root = record.record_lineage_root;
    let durable_record_revision = record.durable_record_revision;
    let durable_lineage_root = record.durable_lineage_root;
    let durable_canonical_digest = record.durable_canonical_digest;
    let settlement_obligation = record.settlement_obligation.clone();
    *record = replacement;
    record.record_revision = record_revision;
    record.record_lineage_root = record_lineage_root;
    record.durable_record_revision = durable_record_revision;
    record.durable_lineage_root = durable_lineage_root;
    record.durable_canonical_digest = durable_canonical_digest;
    record.settlement_obligation = settlement_obligation;
}

fn terminal_result_digest(
    result: &Result<ProductRunOutcome, peritus_product_runner::ProductRunnerError>,
) -> Sha256Digest {
    let mut bytes = b"peritus-product-run-terminal-result-v1\0".to_vec();
    match result {
        Ok(outcome) => {
            bytes.push(match outcome.settlement().disposition() {
                RunDisposition::Accepted => 1,
                RunDisposition::WaitingForUser => 2,
                RunDisposition::RecoveryRequired => 3,
                RunDisposition::Cancelled => 4,
                RunDisposition::CandidateAvailable => 5,
                RunDisposition::FailedNoCandidate => 6,
            });
            if let Some(detail) = outcome.detail() {
                bytes.extend_from_slice(detail.as_bytes());
            }
            for remaining in outcome.remaining_work() {
                bytes.extend_from_slice(&(remaining.len() as u64).to_be_bytes());
                bytes.extend_from_slice(remaining.as_bytes());
            }
            if let Some(candidate) = outcome.candidate() {
                bytes.extend_from_slice(candidate.summary.as_bytes());
                bytes.extend_from_slice(candidate.diff.as_bytes());
            }
        }
        Err(error) => {
            bytes.push(0);
            bytes.extend_from_slice(error.operation().as_bytes());
            bytes.extend_from_slice(error.detail().as_bytes());
        }
    }
    peritus_codec::sha256(&bytes)
}

fn recovered_goal_binding_matches(
    root: &ObligationRoot,
    binding: Option<&crate::product_control::GoalSemanticBinding>,
) -> bool {
    match binding {
        Some(binding) => {
            root.goal == binding.goal().into_bytes()
                && root.goal_attempt == u64::from(binding.attempt())
                && root.goal_user_revision == binding.user_revision()
                && root.required_input_generation == Some(binding.input_generation())
                && root.conversation == binding.conversation().into_bytes()
                && root.run == binding.run()
        }
        None => {
            root.goal == [0; 16]
                && root.goal_attempt == 0
                && root.goal_user_revision == 0
                && root.required_input_generation.is_none()
        }
    }
}

fn retain_pending_projection_failure(record: &mut super::super::RunRecord, detail: &str) {
    record.candidate_actionable = false;
    detail.clone_into(&mut record.interruption_cause);
    if let Ok(snapshot) = replace_snapshot(
        &record.snapshot,
        ProductRunPhase::RecoveryRequired,
        "Completed outcome projection requires recovery",
        detail,
    ) {
        record.snapshot = snapshot;
    }
}
