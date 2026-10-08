//! Fresh tool-capable, read-only independent review turns.

use peritus_agent::{DeveloperLoopLimits, DeveloperLoopRequest};
use peritus_review::ProductReviewSubmission;

use crate::budget::RunAccounting;
use crate::developer_tools::{GroundingEvidence, WorkspaceDeveloperTools, read_only_definitions};
use crate::execution::{ProductRunInput, check_cancelled};
use crate::{ProductRunnerError, ProductRunnerErrorKind, review, turn};

mod evidence;
mod submission;

const MAX_REVIEWER_TURNS: u16 = 32;
const MAX_REVIEWER_TOOL_CALLS: u32 = 256;

pub struct ReviewEvidence<'a> {
    pub candidate: peritus_run_settlement::CandidateIdentity,
    pub conversation_revision: u64,
    pub conversation: &'a str,
    pub diff: &'a str,
    pub gates: &'a str,
    pub developer_commands: &'a str,
    pub prior: &'a str,
    pub reconciliation: &'a str,
    pub coverage_cursor: &'a str,
}

/// Runs a fresh reviewer with bounded read-only workspace tools and parses its typed submission.
pub async fn complete(
    input: &ProductRunInput,
    cycle: u32,
    review_cycle: u32,
    evidence: ReviewEvidence<'_>,
    accounting: &mut RunAccounting,
) -> Result<Option<ProductReviewSubmission>, ProductRunnerError> {
    let mut providers =
        crate::failover::ProviderCursor::new(&input.providers.reviewer, &input.providers.fallbacks);
    let mut correction = None;
    let memory = input.working_memory_async("reviewer").await?;
    let mut rejected_reviews = 0_u64;
    let mut provider_recovery = crate::failover::RoleRecovery::default();
    let mut invocation = 0_u64;
    let evidence_revision = evidence.conversation_revision;
    let grounding_prefix = format!(
        "{}-revision-{evidence_revision}-invocation-",
        turn::request_name(input.run_id, "reviewer", cycle),
    );
    let mut rejected_submission = None;
    loop {
        check_cancelled(input)?;
        if input.conversation.revision() != evidence_revision {
            return Ok(None);
        }
        invocation = invocation.checked_add(1).ok_or_else(|| {
            ProductRunnerError::new(
                ProductRunnerErrorKind::Repository,
                "allocate reviewer invocation identity",
                "invocation sequence overflow",
            )
        })?;
        let revision = input.conversation.revision();
        if revision != evidence_revision {
            return Ok(None);
        }
        let sources = evidence::ReviewSources::new(
            input,
            evidence.candidate,
            evidence_revision,
            review_cycle,
            [
                ("conversation", evidence.conversation),
                ("diff", evidence.diff),
                ("gates", evidence.gates),
                ("developer", evidence.developer_commands),
                ("prior", evidence.prior),
                ("rejected", rejected_submission.as_deref().unwrap_or_default()),
            ],
        )
        .map_err(|error| turn::developer_error(&error))?;
        let submission_handle = sources.submission_handle();
        let mut submission = submission::SubmissionDraft::new(
            submission_handle.clone(),
            review_cycle,
            Some(evidence.coverage_cursor),
        );
        let receipt_policy = crate::trace::ScopedToolReceiptPolicy::new(
            sources
                .submission_scope(&grounding_prefix)
                .map_err(|error| turn::developer_error(&error))?,
            grounding_prefix.clone(),
            submission_handle.clone(),
            &[submission::APPEND_TOOL, submission::FINISH_TOOL],
        )
        .map_err(|error| turn::developer_error(&error))?;
        let mut grounding = GroundingEvidence::for_workspace(&input.workspace_root);
        let mut evidence_progress = evidence::EvidenceProgress::default();
        crate::trace::replay_scoped_tool_observations(
            &input.trace_path,
            &receipt_policy,
            &mut |call, output| {
                grounding.record_completed(call, output);
                sources.recover(call, output, &mut evidence_progress);
                submission::recover_observation(call, output, &mut submission)
            },
        )
        .map_err(|error| turn::developer_error(&error))?;
        if let Some(recovered) = submission.finished()
            && grounding.validate().is_ok()
        {
            check_cancelled(input)?;
            if input.conversation.revision() != evidence_revision {
                return Ok(None);
            }
            return Ok(Some(recovered));
        }
        let mut catalog = sources.catalog(&evidence_progress);
        catalog.push_str(evidence.reconciliation);
        catalog.push_str(&submission.catalog());
        let request = prepare_request(
            input,
            &evidence,
            providers.current().profile().limits().max_input_tokens(),
            correction.as_deref(),
            accounting.remaining(),
            memory.as_ref(),
            &catalog,
        )?;
        let media = match input.media(evidence.conversation, providers.current().profile()) {
            Ok(media) => media,
            Err(error) if let Some(switch) = providers.advance_for_capability(&error) => {
                crate::failover::record_switch(input, "reviewer", cycle, accounting, switch)?;
                correction = Some(crate::failover::RoleRecovery::transfer_correction(
                    correction.as_deref(),
                ));
                continue;
            }
            Err(error) => return Err(error),
        };
        let (prompt, attachments) = media.into_parts(request.prompt);
        let mut tools = input.configure_tools(
            WorkspaceDeveloperTools::read_only(input.workspace_root.clone())
                .with_observational_commands(input.command_runtime.clone())
                .with_grounding(grounding.clone())
                .with_task_contract(evidence.conversation),
        );
        let reopened = memory
            .as_ref()
            .map(|memory| memory.pending_reentry_prefix(&grounding_prefix))
            .transpose()
            .map_err(|error| turn::developer_error(&error))?
            .flatten();
        let request_prefix = match reopened {
            Some(prefix) => prefix,
            None => turn::invocation_request_name(
                input.run_id,
                "reviewer",
                cycle,
                revision,
                invocation,
            )?,
        };
        let mut evidence_tools = evidence::EvidenceTools::new(
            &mut tools,
            &sources,
            &mut evidence_progress,
            memory.is_some(),
        );
        let mut submission_tools = submission::SubmissionTools::new(
            &mut evidence_tools,
            &mut submission,
            memory.is_some(),
        );
        let result = crate::local_context::run_live_invocation_with_receipts(
            providers.current(),
            DeveloperLoopRequest {
                local_session_directory: Some(input.native_session_directory("reviewer")),
                request_prefix,
                system: request.system,
                prompt,
                attachments,
                tools: request.tools,
                limits: reviewer_limits()?,
                cancellation: input.provider_cancellation.clone(),
            },
            &mut submission_tools,
            crate::local_context::InvocationAccounting {
                trace_path: &input.trace_path,
                accounting,
            }
            .with_scoped_tool_receipts(receipt_policy.clone()),
            memory.as_ref(),
            input.conversation.interaction(),
            peritus_agent::DeveloperModelRole::Reviewer,
        )
        .await;
        let staged_submission = submission_tools.finished_this_invocation();
        drop(submission_tools);
        drop(evidence_tools);
        accounting.check()?;
        let result = match result {
            Ok(result) => result,
            Err(error)
                if matches!(
                    &error,
                    peritus_agent::DeveloperLoopError::Trace(_)
                        | peritus_agent::DeveloperLoopError::Context(_)
                ) => {
                // The tool observation is durably synced before local-context publication. A
                // failure after that sync must recover the exact receipt instead of discarding a
                // completed submission. Rebuild from the trace rather than trusting the mutable
                // in-memory draft, which may also contain an execution that was never recorded.
                check_cancelled(input)?;
                let recovered = recover_durable_submission(
                    input,
                    &sources,
                    &receipt_policy,
                    &submission_handle,
                    review_cycle,
                    evidence.coverage_cursor,
                )?;
                if let Some(recovered) = recovered {
                    if input.conversation.revision() != evidence_revision {
                        return Ok(None);
                    }
                    return Ok(Some(recovered));
                }
                drop(sources);
                return Err(turn::developer_error(&error));
            }
            Err(peritus_agent::DeveloperLoopError::SegmentExhausted) => {
                drop(sources);
                accounting.record_role_retry()?;
                provider_recovery.reset();
                correction = Some(crate::failover::RoleRecovery::correction("segment_boundary"));
                continue;
            }
            Err(error) => {
                drop(sources);
                if let Some(reason) = provider_recovery.retry(&error) {
                    accounting.record_role_retry()?;
                    correction = Some(crate::failover::RoleRecovery::correction(reason));
                    continue;
                }
                if let Some(switch) = providers.advance(&error) {
                    crate::failover::record_switch(input, "reviewer", cycle, accounting, switch)?;
                    provider_recovery.reset();
                    correction = Some(crate::failover::RoleRecovery::transfer_correction(
                        correction.as_deref(),
                    ));
                    continue;
                }
                return Err(turn::developer_error(&error));
            }
        };
        drop(sources);
        crate::failover::record_provider_success(&mut provider_recovery);
        check_cancelled(input)?;
        if input.conversation.revision() != evidence_revision {
            return Ok(None);
        }
        let submission = staged_submission.map_or_else(
            || {
                grounded_submission(
                    &tools,
                    &result.text,
                    review_cycle,
                    evidence.coverage_cursor,
                )
            },
            |submission| {
                grounded_staged_submission(&tools, submission, evidence.coverage_cursor)
            },
        );
        match submission {
            Ok(submission) => return Ok(Some(submission)),
            Err(rejected) => {
                rejected_reviews = rejected_reviews.saturating_add(1);
                correction =
                    Some(record_rejected_review(input, rejected_reviews, &rejected, accounting)?);
                rejected_submission = Some(result.text);
            }
        }
    }
}

fn recover_durable_submission(
    input: &ProductRunInput,
    sources: &evidence::ReviewSources<'_>,
    receipt_policy: &crate::trace::ScopedToolReceiptPolicy,
    submission_handle: &str,
    review_cycle: u32,
    coverage_cursor: &str,
) -> Result<Option<ProductReviewSubmission>, ProductRunnerError> {
    let mut submission = submission::SubmissionDraft::new(
        submission_handle.to_owned(),
        review_cycle,
        Some(coverage_cursor),
    );
    let mut grounding = GroundingEvidence::for_workspace(&input.workspace_root);
    let mut evidence_progress = evidence::EvidenceProgress::default();
    crate::trace::replay_scoped_tool_observations(
        &input.trace_path,
        receipt_policy,
        &mut |call, output| {
            grounding.record_completed(call, output);
            sources.recover(call, output, &mut evidence_progress);
            submission::recover_observation(call, output, &mut submission)
        },
    )
    .map_err(|error| turn::developer_error(&error))?;
    if grounding.validate().is_ok() {
        Ok(submission.finished())
    } else {
        Ok(None)
    }
}

struct ReviewRequest {
    system: String,
    prompt: String,
    tools: Vec<peritus_model_protocol::ToolDefinition>,
}

fn prepare_request(
    input: &ProductRunInput,
    evidence: &ReviewEvidence<'_>,
    max_input_tokens: u64,
    correction: Option<&str>,
    remaining: Option<std::time::Duration>,
    memory: Option<&crate::local_context::LocalContextHandle>,
    catalog: &str,
) -> Result<ReviewRequest, ProductRunnerError> {
    let system = turn::reviewer_system(remaining) + input.delivery_instructions();
    let mut tools = read_only_definitions()?;
    tools.push(evidence::definition().map_err(|error| turn::developer_error(&error))?);
    tools.extend(submission::definitions().map_err(|error| turn::developer_error(&error))?);
    let mut budget_tools = tools.clone();
    if let Some(memory) = memory {
        budget_tools
            .extend(memory.tool_definitions().map_err(|error| turn::developer_error(&error))?);
    }
    let prompt = turn::reviewer_user_with_catalog(&turn::ReviewerPrompt {
        system: &system,
        tools: &budget_tools,
        transcript: &input.conversation.stable_request_context(),
        diff: evidence.diff,
        gates: evidence.gates,
        developer_evidence: evidence.developer_commands,
        prior: evidence.prior,
        max_input_tokens,
        delivery: turn::ReviewDelivery {
            scope: input.delivery_scope,
            effect_requirement: crate::delivery_requirement::ExternalEffectRequirement::from_task(
                input.delivery_scope,
                &input.task,
            ),
        },
        correction,
    }, catalog)
    .map_err(|error| turn::developer_error(&error))?;
    Ok(ReviewRequest { system, prompt, tools })
}

fn grounded_submission(
    tools: &WorkspaceDeveloperTools,
    text: &str,
    cycle: u32,
    coverage_cursor: &str,
) -> Result<ProductReviewSubmission, RejectedReview> {
    tools.grounding().validate().map_err(|detail| RejectedReview {
        error: grounding(detail),
        reason: peritus_agent::DeveloperReviewRetryReason::MissingGrounding,
    })?;
    let submission = review::parse(text, cycle).map_err(|error| RejectedReview {
        error,
        reason: peritus_agent::DeveloperReviewRetryReason::InvalidSubmission,
    })?;
    validate_coverage(&submission, coverage_cursor)?;
    Ok(submission)
}

fn grounded_staged_submission(
    tools: &WorkspaceDeveloperTools,
    submission: ProductReviewSubmission,
    coverage_cursor: &str,
) -> Result<ProductReviewSubmission, RejectedReview> {
    tools.grounding().validate().map_err(|detail| RejectedReview {
        error: grounding(detail),
        reason: peritus_agent::DeveloperReviewRetryReason::MissingGrounding,
    })?;
    validate_coverage(&submission, coverage_cursor)?;
    Ok(submission)
}

fn validate_coverage(
    submission: &ProductReviewSubmission,
    coverage_cursor: &str,
) -> Result<(), RejectedReview> {
    if submission.coverage_cursor().is_some_and(|value| value != coverage_cursor) {
        return Err(RejectedReview {
            error: ProductRunnerError::new(
                ProductRunnerErrorKind::InvalidModelOutput,
                "validate indexed reviewer coverage",
                "review coverage cursor does not match the current finding page",
            ),
            reason: peritus_agent::DeveloperReviewRetryReason::InvalidSubmission,
        });
    }
    Ok(())
}

struct RejectedReview {
    error: ProductRunnerError,
    reason: peritus_agent::DeveloperReviewRetryReason,
}

fn record_rejected_review(
    input: &ProductRunInput,
    rejected_reviews: u64,
    rejected: &RejectedReview,
    accounting: &mut RunAccounting,
) -> Result<String, ProductRunnerError> {
    accounting.record_role_retry()?;
    if let Some(port) = input.conversation.interaction() {
        port.observe(peritus_agent::DeveloperActivity::ReviewRetry {
            next_attempt: rejected_reviews.saturating_add(1),
            reason: rejected.reason,
        })
        .map_err(|error| turn::developer_error(&error))?;
    }
    Ok(match rejected.reason {
        peritus_agent::DeveloperReviewRetryReason::MissingGrounding => {
            correction_prompt(&rejected.error)
        }
        peritus_agent::DeveloperReviewRetryReason::InvalidSubmission => {
            submission_correction_prompt(&rejected.error)
        }
    })
}

fn correction_prompt(error: &ProductRunnerError) -> String {
    format!(
        "The previous review was rejected during {}: {}. Supply the missing repository evidence identified by that diagnostic through the declared host tools, then finish the retained incremental typed submission or return one complete typed reviewer JSON object. Retained observations and staged fields from unchanged source remain valid; do not repeat an inventory, read, or field that already supplies the required evidence.",
        error.operation(),
        error.detail(),
    )
}

fn submission_correction_prompt(error: &ProductRunnerError) -> String {
    format!(
        "Repository grounding was retained. The previous typed review was rejected during {}: {}. Repair only the submission field or schema element identified by that diagnostic, preserving every finding and its evidence. Reuse retained incremental fields and finish them through the declared host tools, or return one complete typed reviewer JSON object. A submission-format correction does not require another workspace inventory, repetition of unchanged reads, or re-emission of already accepted fields; inspect additional source only when a candidate fact changed or specific evidence is still missing.",
        error.operation(),
        error.detail(),
    )
}

fn reviewer_limits() -> Result<DeveloperLoopLimits, ProductRunnerError> {
    DeveloperLoopLimits::new(MAX_REVIEWER_TURNS, MAX_REVIEWER_TOOL_CALLS)
        .map(DeveloperLoopLimits::with_provider_output)
        .map_err(|error| turn::developer_error(&error))
}

fn grounding(detail: &'static str) -> ProductRunnerError {
    ProductRunnerError::new(
        ProductRunnerErrorKind::InvalidModelOutput,
        "ground independent review in repository evidence",
        detail,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ungrounded_review_has_a_typed_public_reason_without_response_bytes() {
        let root = tempfile::tempdir().expect("workspace");
        let tools = WorkspaceDeveloperTools::read_only(root.path().to_owned());
        let Err(rejected) =
            grounded_submission(&tools, "PRIVATE_RESPONSE_CANARY", 1, "coverage")
        else {
            panic!("uninspected review cannot be accepted")
        };
        assert_eq!(rejected.reason, peritus_agent::DeveloperReviewRetryReason::MissingGrounding);
        assert!(!rejected.error.detail().contains("PRIVATE_RESPONSE_CANARY"));
    }

    #[test]
    fn rejected_review_requires_fresh_authoritative_reads() {
        let error = grounding("repository grounding requires a successful workspace listing");
        let correction = correction_prompt(&error);

        assert!(correction.contains("fresh workspace_list call"));
        assert!(correction.contains("host tool-call interface"));
        assert!(correction.contains("authoritative source inputs"));
        assert!(correction.contains("exact changed files"));
        assert!(correction.contains(error.detail()));
    }
}
