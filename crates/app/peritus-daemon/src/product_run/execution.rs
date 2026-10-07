//! Active product-run task ownership and terminal projection.
mod runtime;

use peritus_app_protocol::{ProductRunPhase, ProductRunSnapshot};
use peritus_product_runner::ProductRunOutcome;
use peritus_product_runner::control::GoalSettlement;
use peritus_run_settlement::RunDisposition;
use peritus_types::RunId;
use std::sync::Arc;

use super::publication::{MutationDisposition, RunMutationKind};
use super::snapshot::replace_snapshot;
use super::{ProductRunService, ProductRunServiceError};
mod baseline;
mod failure;
mod goal;
mod handoff;
mod launch;
mod terminal;
#[cfg(test)]
pub use launch::inject_finish_barrier;

impl ProductRunService {
    fn observe(&self, run_id: RunId, update: peritus_product_runner::ProductRunUpdate) {
        let rejected_head = peritus_codec::sha256(update.finding_state.as_bytes());
        let rejected_bytes = u64::try_from(update.finding_state.len()).unwrap_or(u64::MAX);
        let rejected_phase = match update.phase {
            peritus_product_runner::ProductRunPhase::Designing => 1,
            peritus_product_runner::ProductRunPhase::Writing => 2,
            peritus_product_runner::ProductRunPhase::Checking => 3,
            peritus_product_runner::ProductRunPhase::Reviewing => 4,
            peritus_product_runner::ProductRunPhase::Fixing => 5,
            peritus_product_runner::ProductRunPhase::Verifying => 6,
            peritus_product_runner::ProductRunPhase::Finalizing => 7,
            peritus_product_runner::ProductRunPhase::Complete => 8,
        };
        let finding_catalog =
            super::persistence::FindingBodyStore::published_catalog(&update.finding_state);
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
        let attempt = match self.capture_run_identity(run_id) {
            Ok(identity) => identity.cancelled,
            Err(_) => return,
        };
        let input_digest = rejected_head;
        let mutation = self.mutate_run(
            run_id,
            Some(&attempt),
            RunMutationKind::ExecutionObservation,
            input_digest,
            MutationDisposition::DurabilityRequired,
            |record| {
                let mut rejected_handoff = None;
                if record.handoff_recovery_pending {
                    return Ok(None);
                }
                let retain_cancelled = record.user_cancelled
                    || record.snapshot.phase() == ProductRunPhase::Cancelled;
                let retain_rejected_finding = record.rejected_finding_update.is_some();
                record.progress.observe(update.progress);
                if let Some(checkpoint) = update.checkpoint {
                    record.checkpoint = Some(checkpoint);
                }
                record.remaining_work = update.remaining_work;
                if !retain_cancelled
                    && phase != record.snapshot.phase()
                    && update.phase != peritus_product_runner::ProductRunPhase::Finalizing
                    && matches!(
                        record.interaction.mode,
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
                        && record
                            .interaction
                            .append(peritus_app_protocol::ProductActivityKind::Status, message, "")
                            .is_err()
                    {
                        record.interaction.persistence_failed.store(
                            true,
                            std::sync::atomic::Ordering::Release,
                        );
                        return Ok(None);
                    }
                }
                let projected_phase = if retain_cancelled {
                    ProductRunPhase::Cancelled
                } else {
                    phase
                };
                if let Ok(operation) =
                    super::operation::retained_execution(run_id, projected_phase, "")
                    && let Ok(snapshot) = ProductRunSnapshot::new(
                        run_id,
                        record.request.workspace_id(),
                        record.request.providers(),
                        projected_phase,
                        update.cycle,
                        record.request.display_task().to_owned(),
                        update.status.clone(),
                        update.diff.clone(),
                        update.gates.clone(),
                        update.review.clone(),
                        update.summary.clone(),
                        operation,
                    )
                {
                    record.snapshot = snapshot;
                }
                if !update.finding_state.is_empty() && !retain_rejected_finding {
                    match finding_catalog {
                        Ok(catalog) => {
                            record.finding_state = update.finding_state;
                            record.finding_catalog = catalog;
                            record.rejected_finding_update = None;
                        }
                        Err(error) => {
                            let accepted_head = record.finding_catalog.head_digest;
                            let detail = error.describe();
                            record.rejected_finding_update = Some(super::RejectedFindingUpdate {
                                accepted_head,
                                rejected_head,
                                rejected_bytes,
                                rejected_finding_state: update.finding_state,
                                phase: rejected_phase,
                                cycle: update.cycle,
                                status: update.status,
                                diff: update.diff,
                                gates: update.gates,
                                review: update.review,
                                summary: update.summary,
                                error: detail.clone(),
                            });
                            record.interruption_cause.clone_from(&detail);
                            record.candidate_actionable = false;
                            record.handoff_recovery_pending = true;
                            if let Ok(snapshot) = replace_snapshot(
                                &record.snapshot,
                                if record.user_cancelled {
                                    ProductRunPhase::Cancelled
                                } else {
                                    ProductRunPhase::RecoveryRequired
                                },
                                if record.user_cancelled {
                                    "Run cancelled"
                                } else {
                                    "Product finding update requires recovery"
                                },
                                if record.user_cancelled {
                                    "Cancelled while rejected finding evidence was retained"
                                } else {
                                    &detail
                                },
                            ) {
                                record.snapshot = snapshot;
                            }
                            let Some(sequence) = record.handoff_sequence.checked_add(1) else {
                                record.interaction.record_persistence_failure(
                                    "product-run handoff sequence overflow".to_owned(),
                                );
                                return Ok(None);
                            };
                            let mut handoff = record.clone();
                            handoff.handoff_sequence = sequence;
                            let cancellation_generation = record
                                .user_cancelled
                                .then(|| {
                                    self.pending_user_cancellation_generation(
                                        run_id,
                                        &record.cancelled,
                                    )
                                })
                                .flatten();
                            rejected_handoff = Some((
                                handoff,
                                record.handoff_sequence,
                                record.attempt_sequence,
                                Arc::clone(&record.cancelled),
                                rejected_head,
                                cancellation_generation,
                            ));
                        }
                    }
                }
                Ok(rejected_handoff)
            },
        );
        let (rejected_handoff, ticket) = match mutation {
            Ok(value) => value,
            Err(_) => return,
        };
        if self.await_run_durable(ticket).is_err() {
            return;
        }

        let Some((
            mut handoff,
            prior_sequence,
            attempt_sequence,
            attempt,
            rejected_head,
            cancellation_generation,
        )) = rejected_handoff
        else {
            return;
        };
        if super::persistence::write_handoff_retrying(
            &self.inner.directory,
            &handoff,
            super::persistence::HandoffKind::RejectedFindingUpdate,
            &self.inner.control_shutdown,
        )
        .is_err()
        {
            return;
        }
        handoff.handoff_recovery_pending = false;
        let installed = self.mutate_run(
            run_id,
            Some(&attempt),
            RunMutationKind::Recovery,
            rejected_head,
            MutationDisposition::DurabilityRequired,
            move |record| {
                if record.attempt_sequence != attempt_sequence
                    || record.handoff_sequence != prior_sequence
                    || !record.rejected_finding_update.as_ref().is_some_and(|rejected| {
                        rejected.rejected_head == rejected_head
                    })
                {
                    return Ok(None);
                }
                handoff.user_cancelled |= record.user_cancelled;
                if handoff.user_cancelled {
                    handoff.snapshot = replace_snapshot(
                        &handoff.snapshot,
                        ProductRunPhase::Cancelled,
                        "Run cancelled",
                        "Cancelled while rejected finding evidence was retained",
                    )
                    .unwrap_or_else(|_| record.snapshot.clone());
                }
                handoff.interaction = record.interaction.clone();
                handoff.preview = record.preview.clone();
                let record_revision = record.record_revision;
                let record_lineage_root = record.record_lineage_root;
                let durable_record_revision = record.durable_record_revision;
                let durable_lineage_root = record.durable_lineage_root;
                let durable_canonical_digest = record.durable_canonical_digest;
                let settlement_obligation = record.settlement_obligation.clone();
                *record = handoff;
                record.record_revision = record_revision;
                record.record_lineage_root = record_lineage_root;
                record.durable_record_revision = durable_record_revision;
                record.durable_lineage_root = durable_lineage_root;
                record.durable_canonical_digest = durable_canonical_digest;
                record.settlement_obligation = settlement_obligation;
                record.interaction.record_persistence_failure(
                    "the runner emitted a finding ledger that was not fully externalized".to_owned(),
                );
                record.interaction.persistence_failed.store(
                    true,
                    std::sync::atomic::Ordering::Release,
                );
                let control = record.control_cancellation.clone();
                let provider = record.provider_cancellation.clone();
                let cancellation_handoff = if record.user_cancelled {
                    let sequence = record.handoff_sequence.checked_add(1).ok_or_else(|| {
                        ProductRunServiceError::internal(
                            "publish terminal cancellation handoff",
                            "handoff sequence overflow",
                        )
                    })?;
                    let mut cancellation = record.clone();
                    cancellation.handoff_sequence = sequence;
                    cancellation.snapshot = replace_snapshot(
                        &cancellation.snapshot,
                        ProductRunPhase::Cancelled,
                        "Run cancelled",
                        "Cancelled while rejected finding evidence was retained",
                    )?;
                    cancellation.handoff_recovery_pending = true;
                    record.handoff_recovery_pending = true;
                    Some((
                        cancellation,
                        record.handoff_sequence,
                        record.attempt_sequence,
                        Arc::clone(&record.cancelled),
                        self.pending_user_cancellation_generation(run_id, &record.cancelled),
                    ))
                } else {
                    None
                };
                Ok(Some((cancellation_handoff, control, provider)))
            },
        );
        let (installed, ticket) = match installed {
            Ok(value) => value,
            Err(_) => return,
        };
        if self.await_run_durable(ticket).is_err() {
            return;
        }
        let Some((cancellation_handoff, control, provider)) = installed else {
            return;
        };
        if let Some(generation) = cancellation_generation {
            let _ = self.acknowledge_user_cancellation(run_id, &attempt, generation);
        }
        attempt.store(true, std::sync::atomic::Ordering::Release);
        control.cancel();
        let _ = provider.cancel();
        let Some((
            mut cancellation,
            prior_sequence,
            attempt_sequence,
            attempt,
            cancellation_generation,
        )) = cancellation_handoff
        else {
            return;
        };
        if super::persistence::write_handoff_retrying(
            &self.inner.directory,
            &cancellation,
            super::persistence::HandoffKind::Terminal,
            &self.inner.control_shutdown,
        )
        .is_err()
        {
            return;
        }
        cancellation.handoff_recovery_pending = false;
        let input = peritus_codec::sha256(b"terminal-cancellation-handoff");
        let installed = self.mutate_run(
            run_id,
            Some(&attempt),
            RunMutationKind::Recovery,
            input,
            MutationDisposition::DurabilityRequired,
            move |record| {
                if record.attempt_sequence != attempt_sequence
                    || record.handoff_sequence != prior_sequence
                    || !record.user_cancelled
                {
                    return Ok(false);
                }
                cancellation.user_cancelled = true;
                cancellation.interaction = record.interaction.clone();
                cancellation.preview = record.preview.clone();
                let record_revision = record.record_revision;
                let record_lineage_root = record.record_lineage_root;
                let durable_record_revision = record.durable_record_revision;
                let durable_lineage_root = record.durable_lineage_root;
                let durable_canonical_digest = record.durable_canonical_digest;
                let settlement_obligation = record.settlement_obligation.clone();
                *record = cancellation;
                record.record_revision = record_revision;
                record.record_lineage_root = record_lineage_root;
                record.durable_record_revision = durable_record_revision;
                record.durable_lineage_root = durable_lineage_root;
                record.durable_canonical_digest = durable_canonical_digest;
                record.settlement_obligation = settlement_obligation;
                Ok(true)
            },
        );
        let (installed, ticket) = match installed {
            Ok(value) => value,
            Err(_) => return,
        };
        if self.await_run_durable(ticket).is_ok()
            && installed
            && let Some(generation) = cancellation_generation
        {
            let _ = self.acknowledge_user_cancellation(run_id, &attempt, generation);
        }
    }
}
