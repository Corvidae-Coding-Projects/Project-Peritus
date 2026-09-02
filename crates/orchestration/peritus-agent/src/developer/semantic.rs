//! Model-authored semantic checkpoints for long developer transcripts.

use peritus_model_protocol::{
    BoundedText, ContentBlock, Message, ProtocolLimits, ProviderProfile, ReducedItem, Role,
    ToolDefinition,
};

use crate::ModelSession;

use super::{
    DeveloperContextCompaction, DeveloperLoopError,
    context::{
        COMPACTION_OPEN, CompactionCandidate, compaction_candidate, context_thresholds, digest_hex,
    },
    context_encoding::{POLICY_REVISION, encode_source, estimate_bytes, estimated_request_tokens},
};

const RETAIN_RECENT_MESSAGES: usize = 8;
const CHECKPOINT_PROMPT: &str = "Create a concise context checkpoint for the coding agent that will continue this same task. Preserve completed work and observed state; key decisions and constraints; user preferences; failures and exact diagnostics; unresolved work and concrete next steps; and important paths, commands, identifiers, or examples. Use only evidence in this conversation, distinguish completed work from plans, and do not call tools. Return plain text only.";

/// One prepared semantic-compaction request and its exact replacement source.
pub(super) struct SemanticCompaction {
    candidate: CompactionCandidate,
    request_messages: Vec<Message>,
}

impl SemanticCompaction {
    pub(super) fn prepare(
        messages: &[Message],
        tools: &[ToolDefinition],
        profile: &ProviderProfile,
        reserved_output_tokens: u64,
        limits: ProtocolLimits,
    ) -> Result<Option<Self>, DeveloperLoopError> {
        let (usable_input, trigger) = context_thresholds(profile, reserved_output_tokens)?;
        let estimated = estimated_request_tokens(messages, tools);
        if estimated <= trigger {
            return Ok(None);
        }
        let candidate = match compaction_candidate(messages, RETAIN_RECENT_MESSAGES)? {
            Some(candidate) => candidate,
            None if estimated > usable_input => {
                let Some(candidate) = compaction_candidate(messages, 0)? else {
                    return Ok(None);
                };
                candidate
            }
            None => return Ok(None),
        };
        let mut request_messages = messages.iter().take(2).cloned().collect::<Vec<_>>();
        request_messages.extend_from_slice(candidate.source());
        request_messages.push(Message::new(
            Role::User,
            vec![ContentBlock::Text(BoundedText::new(CHECKPOINT_PROMPT.to_owned(), limits)?)],
            limits,
        )?);
        Ok(Some(Self { candidate, request_messages }))
    }

    pub(super) fn request_messages(&self) -> &[Message] {
        &self.request_messages
    }

    pub(super) fn install(
        self,
        messages: &mut Vec<Message>,
        session: &ModelSession,
        limits: ProtocolLimits,
    ) -> Result<Option<DeveloperContextCompaction>, DeveloperLoopError> {
        let mut summary = String::new();
        for item in session.completed_items() {
            match item {
                ReducedItem::Text { text, .. } => summary.push_str(text.expose_for_wire()),
                ReducedItem::Refusal { .. } => return Err(DeveloperLoopError::Refused),
                ReducedItem::ToolCall { .. } => {
                    return Err(DeveloperLoopError::Context(
                        "semantic compaction returned an unexpected tool call".to_owned(),
                    ));
                }
                ReducedItem::Reasoning { .. }
                | ReducedItem::Structured { .. }
                | ReducedItem::ProviderNative { .. } => {}
            }
        }
        if summary.trim().is_empty() {
            return Err(DeveloperLoopError::EmptyResponse);
        }
        self.install_summary(messages, &summary, limits)
    }

    fn install_summary(
        self,
        messages: &mut Vec<Message>,
        summary: &str,
        limits: ProtocolLimits,
    ) -> Result<Option<DeveloperContextCompaction>, DeveloperLoopError> {
        let candidate = self.candidate;
        let current = messages.get(candidate.start..candidate.end).ok_or_else(|| {
            DeveloperLoopError::Context(
                "semantic compaction source moved before installation".into(),
            )
        })?;
        if peritus_codec::sha256(&encode_source(current)?) != candidate.source_digest {
            return Err(DeveloperLoopError::Context(
                "semantic compaction source changed before installation".into(),
            ));
        }
        let replacement = render_checkpoint(summary, candidate.source_digest);
        let replacement_tokens = estimate_bytes(replacement.len());
        if replacement_tokens >= candidate.replaced_tokens {
            return Ok(None);
        }
        let replacement_digest = peritus_codec::sha256(replacement.as_bytes());
        let source_messages = u16::try_from(candidate.source.len()).map_err(|_| {
            DeveloperLoopError::Context("compaction source count exceeded u16".to_owned())
        })?;
        let record = DeveloperContextCompaction::new(
            [peritus_codec::sha256(POLICY_REVISION), candidate.source_digest, replacement_digest],
            source_messages,
            candidate.replaced_tokens,
            replacement_tokens,
        );
        let message = Message::new(
            Role::User,
            vec![ContentBlock::Text(BoundedText::new(replacement, limits)?)],
            limits,
        )?;
        messages.splice(candidate.start..candidate.end, [message]);
        Ok(Some(record))
    }
}

fn render_checkpoint(summary: &str, source_digest: peritus_types::Sha256Digest) -> String {
    let policy = digest_hex(peritus_codec::sha256(POLICY_REVISION));
    let source = digest_hex(source_digest);
    let mut output = format!(
        "{COMPACTION_OPEN}version=\"2\" mode=\"semantic\" policy_sha256=\"{policy}\" source_sha256=\"{source}\">\n"
    );
    output.push_str(
        "Non-authoritative semantic checkpoint of earlier completed work. Exact observations remain in the durable trace; use fresh tools when current state matters.\n",
    );
    output.push_str(summary.trim());
    output.push_str("\n</peritus-compaction>");
    output
}
