//! In-place directory conversation with no Git baseline or recursive startup inventory.

use super::{
    ProductRunInput, ProductRunOutcome, ProductRunPhase, ProductRunQuestion, ProductRunUpdate,
    ProductRunner, RunObserver,
};
use crate::{
    ConversationMode, ProductRunnerError, ProductRunnerErrorKind,
    budget::RunAccounting,
    developer_tools::{WorkspaceDeveloperTools, WorkspaceOwnership},
};
use peritus_agent::{DeveloperLoopLimits, DeveloperLoopRequest};
use peritus_run_settlement::{SettlementCause, SettlementReducer};
use std::{path::PathBuf, sync::atomic::Ordering};

impl ProductRunner {
    /// Runs a conversation in an ordinary directory. Requested edits are already in place;
    /// neither completion nor cancellation represents Git candidate acceptance or rollback.
    ///
    /// # Errors
    /// Returns trace/accounting failures, never initializes Git or scans unrelated folder content.
    pub async fn converse_folder(
        input: ProductRunInput,
        mode: ConversationMode,
        writable: bool,
        protected: &[PathBuf],
        observe: RunObserver,
    ) -> Result<ProductRunOutcome, ProductRunnerError> {
        let mut accounting = RunAccounting::direct_folder(input.max_elapsed)?;
        accounting.check()?;
        crate::trace::prepare(&input.trace_path)?;
        let model = if mode == ConversationMode::Review {
            &input.providers.reviewer
        } else {
            &input.providers.writer
        };
        let (request, mut tools) =
            prepare(&input, mode, writable, protected, &accounting, model.as_ref())?;
        let memory = if writable && mode == ConversationMode::Chat {
            crate::local_context::LocalContextHandle::open_folder(&input, protected)?
        } else {
            None
        };
        let result = tokio::time::timeout(
            input.max_elapsed,
            crate::local_context::run_live_invocation(
                model.as_ref(),
                request,
                &mut tools,
                &input.trace_path,
                memory.as_ref(),
                input.conversation.interaction(),
            ),
        )
        .await;
        let (cause, reply, detail) = match result {
            Ok(Ok(result)) => match accounting.record(&result) {
                Ok(()) => (SettlementCause::UserWait, Some(result.text), None),
                Err(error) => (
                    super::settlement::cause_from_error(&error, false),
                    None,
                    Some(error.to_string()),
                ),
            },
            Ok(Err(error)) => {
                let error = crate::turn::developer_error(&error);
                (super::settlement::cause_from_error(&error, false), None, Some(error.to_string()))
            }
            Err(_) => {
                input.cancelled.store(true, Ordering::Release);
                let _ = input.provider_cancellation.cancel();
                (
                    SettlementCause::Deadline,
                    None,
                    Some(
                        "Conversation time limit reached; in-place effects are retained".to_owned(),
                    ),
                )
            }
        };
        observe(ProductRunUpdate {
            phase: ProductRunPhase::Finalizing,
            cycle: 1,
            status: "Finishing folder conversation; requested edits are in place".to_owned(),
            diff: String::new(),
            gates: tools.0.verification_evidence(),
            review: String::new(),
            summary:
                "No Git candidate or automatic rollback; completed in-place effects are retained."
                    .to_owned(),
            finding_state: String::new(),
            progress: accounting.latest_snapshot(),
            checkpoint: None,
            remaining_work: Vec::new(),
        });
        Ok(ProductRunOutcome {
            settlement: SettlementReducer::new()
                .settle(cause)
                .map_err(|_| invalid("folder conversation settlement invariant failed"))?,
            candidate: None,
            question: reply.map(|message| ProductRunQuestion {
                message,
                conversation_revision: input.conversation.incorporated_revision(),
            }),
            detail,
            remaining_work: Vec::new(),
            resume: None,
        })
    }
}

fn invalid(detail: &str) -> ProductRunnerError {
    ProductRunnerError::new(
        ProductRunnerErrorKind::InvalidPrecondition,
        "run folder conversation",
        detail,
    )
}

fn prepare(
    input: &ProductRunInput,
    mode: ConversationMode,
    writable: bool,
    protected: &[PathBuf],
    accounting: &RunAccounting,
    model: &dyn peritus_provider_core::ModelProvider,
) -> Result<(DeveloperLoopRequest, super::conversation::ConversationalTools), ProductRunnerError> {
    let readonly = !writable || mode != ConversationMode::Chat;
    let prefix = crate::turn::request_name(
        input.run_id,
        "folder",
        u32::try_from(input.conversation.revision())
            .map_err(|_| invalid("conversation revision overflow"))?,
    );
    let tools = super::conversation::ConversationalTools(
        if readonly {
            WorkspaceDeveloperTools::read_only(input.workspace_root.clone())
        } else {
            WorkspaceDeveloperTools::with_ownership(
                input.workspace_root.clone(),
                WorkspaceOwnership::direct(),
                input.trace_path.with_extension("effects.bin"),
                prefix.clone(),
                accounting.remaining(),
                input.command_runtime.clone(),
            )
        }
        .with_protected_paths(protected),
    );
    let prompt = input.conversation.render();
    let media = crate::workspace_media::discover_explicit(
        &input.workspace_root,
        &prompt,
        model.profile(),
        protected,
    )?;
    let (prompt, attachments) = media.into_parts(prompt);
    let mut system = super::conversation::system(mode);
    system.push_str("\nThis workspace is an ordinary directory, not a managed Git candidate. Requested file edits happen directly in this folder. Do not initialize Git, make a baseline commit, scan the entire directory, or inspect unrelated private files just to begin a conversation. No automatic Git acceptance, discard or rollback is available. Commands have explicitly authorized raw local-user effects, not a filesystem sandbox. Do not target Peritus private state. Whole-folder size and growth are not measured.");
    if readonly {
        system.push_str("\nThis invocation has read-only authority. Do not perform mutations or invoke processes.");
    }
    system.push_str("\nFor working memory, use explicit file dependencies, conversation or task validity; candidate validity is unavailable because no whole-folder snapshot is taken. Images are attached only by explicit file path; ask for an exact path when needed.");
    let request = DeveloperLoopRequest {
        request_prefix: prefix,
        system,
        prompt,
        attachments,
        tools: if readonly {
            crate::developer_tools::read_only_definitions()?
        } else {
            crate::developer_tools::definitions()?
        },
        limits: DeveloperLoopLimits::new(48, 512)
            .map_err(|error| crate::turn::developer_error(&error))?,
        cancellation: input.provider_cancellation.clone(),
    };
    Ok((request, tools))
}
