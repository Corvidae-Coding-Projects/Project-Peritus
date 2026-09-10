//! Deterministic C6-validated prompt-view reductions over immutable public replies.

use super::{ControlError, ControlText, InvocationId, PublicReplyReference};
use peritus_context::{
    AuthorityClass, CompactionPolicy, CompactionPolicyId, CompactionProposal, ContentKind,
    ContextGraph, ContextLimits, ContextNode, ContextNodeId, ContextNodeMetadata, ContextPlanId,
    Provenance, RequirementMode, RoleVisibility, SelectionPolicy, SourceRange, TokenBudget,
    TrustClass, bind_context_content, select_context, validate_compaction,
};
use peritus_policy::ActorRole;
use peritus_role::{ContextClass, HarnessRole, RoleProfile};
use serde::Deserialize;
use serde::Serialize;
use std::fmt::Write;

const MAX_COMPACTED_REPLIES: usize = 1024;
const MAX_FOCUS_BYTES: usize = 1024;
const MAX_SUMMARY_BYTES: usize = 1024;
const POLICY_LABEL: &[u8] = b"peritus-workbench/deterministic-public-reply-compaction/v1";

/// One exact public reply replaced by a smaller source handle in the active prompt view.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompactedReply {
    invocation: InvocationId,
    source_digest: [u8; 32],
    source_bytes: u64,
    replacement: ControlText<MAX_SUMMARY_BYTES>,
}
impl CompactedReply {
    /// Returns the immutable reply source identity.
    #[must_use]
    pub const fn invocation(&self) -> InvocationId {
        self.invocation
    }
    /// Returns the original exact reply digest.
    #[must_use]
    pub const fn source_digest(&self) -> peritus_types::Sha256Digest {
        peritus_types::Sha256Digest::new(self.source_digest)
    }
    /// Returns the original exact reply byte size.
    #[must_use]
    pub const fn source_bytes(&self) -> u64 {
        self.source_bytes
    }
    /// Borrows the exact deterministic replacement text.
    #[must_use]
    pub fn replacement(&self) -> &str {
        self.replacement.as_str()
    }
}

/// Current atomically published prompt view. Exact reply artifacts remain unchanged.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PromptView {
    generation: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    focus: Option<ControlText<MAX_FOCUS_BYTES>>,
    entries: Vec<CompactedReply>,
}
impl PromptView {
    /// Returns whether no prompt-view generation has been published.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.generation == 0 && self.entries.is_empty() && self.focus.is_none()
    }
    /// Returns the monotonic view generation, independent of input generation.
    #[must_use]
    pub const fn generation(&self) -> u64 {
        self.generation
    }
    /// Borrows the user-selected focus preference, if supplied.
    #[must_use]
    pub fn focus(&self) -> Option<&str> {
        self.focus.as_ref().map(ControlText::as_str)
    }
    /// Borrows canonical source replacements.
    #[must_use]
    pub fn entries(&self) -> &[CompactedReply] {
        &self.entries
    }
    /// Returns a deterministic replacement for a named reply.
    #[must_use]
    pub fn replacement(&self, invocation: InvocationId) -> Option<&str> {
        self.entries
            .binary_search_by_key(&invocation, CompactedReply::invocation)
            .ok()
            .map(|index| self.entries[index].replacement())
    }

    /// Returns whether the deterministic structural handle is strictly smaller than the source.
    ///
    /// # Errors
    /// Rejects an invalid focus preference.
    pub fn source_is_reducible(
        source: &PublicReplyReference,
        focus: Option<&str>,
    ) -> Result<bool, ControlError> {
        let focus = focus.map(|value| ControlText::new(value.to_owned())).transpose()?;
        match deterministic_handle(source, focus.as_ref()) {
            Ok(_) => Ok(true),
            Err(ControlError::Capacity) => Ok(false),
            Err(error) => Err(error),
        }
    }

    /// Builds a local deterministic view and validates every replacement through C6.
    ///
    /// # Errors
    /// Rejects empty/no-saving proposals, malformed focus, source drift, or C6 rejection.
    pub fn compact(
        generation: u64,
        focus: Option<String>,
        sources: &[(PublicReplyReference, String)],
    ) -> Result<Self, ControlError> {
        if generation == 0 || sources.is_empty() || sources.len() > MAX_COMPACTED_REPLIES {
            return Err(ControlError::InvalidInput);
        }
        let focus = focus.map(ControlText::new).transpose()?;
        let mut entries = Vec::with_capacity(sources.len());
        for (index, (reference, text)) in sources.iter().enumerate() {
            if text.len() as u64 != reference.bytes()
                || peritus_codec::sha256(text.as_bytes()) != reference.digest()
            {
                return Err(ControlError::InvalidInput);
            }
            let handle = match deterministic_handle(reference, focus.as_ref()) {
                Ok(handle) => handle,
                Err(ControlError::Capacity) => continue,
                Err(error) => return Err(error),
            };
            validate_c6(reference, text, &handle, index)?;
            entries.push(CompactedReply {
                invocation: reference.after_invocation(),
                source_digest: reference.digest().into_bytes(),
                source_bytes: reference.bytes(),
                replacement: ControlText::new(handle)?,
            });
        }
        entries.sort_by_key(CompactedReply::invocation);
        if entries.is_empty()
            || entries.windows(2).any(|pair| pair[0].invocation == pair[1].invocation)
        {
            return Err(ControlError::InvalidInput);
        }
        Ok(Self { generation, focus, entries })
    }

    pub(super) fn validate(&self, replies: &[PublicReplyReference]) -> Result<(), ControlError> {
        if self.is_empty() {
            return Ok(());
        }
        if self.generation == 0
            || self.entries.is_empty()
            || self.entries.len() > MAX_COMPACTED_REPLIES
            || self.entries.windows(2).any(|pair| pair[0].invocation >= pair[1].invocation)
        {
            return Err(ControlError::InvalidInput);
        }
        for entry in &self.entries {
            let source = replies
                .iter()
                .find(|reply| reply.after_invocation() == entry.invocation)
                .ok_or(ControlError::InvalidInput)?;
            if source.digest().as_bytes() != &entry.source_digest
                || source.bytes() != entry.source_bytes
                || entry.replacement.as_str().len() as u64 >= entry.source_bytes
            {
                return Err(ControlError::InvalidInput);
            }
        }
        Ok(())
    }
}

fn deterministic_handle(
    source: &PublicReplyReference,
    focus: Option<&ControlText<MAX_FOCUS_BYTES>>,
) -> Result<String, ControlError> {
    let mut handle = String::new();
    write!(
        handle,
        "Structural source handle for prior public reply after invocation {} (SHA256 {}, {} bytes). This is not a semantic summary; exact text remains in immutable request history.",
        hex(source.after_invocation().as_bytes()),
        hex(source.digest().as_bytes()),
        source.bytes()
    )
    .map_err(|_| ControlError::Capacity)?;
    if let Some(focus) = focus {
        write!(handle, " User focus preference: {}.", focus.as_str())
            .map_err(|_| ControlError::Capacity)?;
    }
    if handle.len() > MAX_SUMMARY_BYTES || handle.len() as u64 >= source.bytes() {
        return Err(ControlError::Capacity);
    }
    Ok(handle)
}

fn validate_c6(
    source: &PublicReplyReference,
    text: &str,
    summary: &str,
    index: usize,
) -> Result<(), ControlError> {
    let limits =
        ContextLimits::new(4, 1024 * 1024, 2, 2).map_err(|_| ControlError::InvalidInput)?;
    let source_id = context_id(b"source", source.after_invocation().as_bytes(), index)?;
    let output_id = context_id(b"output", source.after_invocation().as_bytes(), index)?;
    let visibility = RoleVisibility::new(vec![ActorRole::Writer], limits)
        .map_err(|_| ControlError::InvalidInput)?;
    let source_content = bind_context_content(text.as_bytes().to_vec(), source.digest(), limits)
        .map_err(|_| ControlError::InvalidInput)?;
    let source_tokens = token_estimate(text.len());
    let metadata = ContextNodeMetadata::new(
        source_id,
        Provenance::Agent,
        AuthorityClass::NonAuthoritative,
        TrustClass::Untrusted,
        ContextClass::AgentProgress,
        ContentKind::AgentProgress,
        source_tokens,
        1,
        RequirementMode::Optional,
        0,
        visibility,
        Vec::new(),
        limits,
    )
    .map_err(|_| ControlError::InvalidInput)?;
    let graph = ContextGraph::new(vec![ContextNode::new(metadata, source_content)], limits)
        .map_err(|_| ControlError::InvalidInput)?;
    let selection = SelectionPolicy::new(
        RoleProfile::for_harness_role(HarnessRole::Writer),
        TokenBudget::new(source_tokens.saturating_add(2), 1, 0)
            .map_err(|_| ControlError::InvalidInput)?,
        2,
        text.len(),
    )
    .map_err(|_| ControlError::InvalidInput)?;
    let plan_id = ContextPlanId::new(peritus_codec::sha256(POLICY_LABEL));
    let plan =
        select_context(&graph, &selection, plan_id).map_err(|_| ControlError::InvalidInput)?;
    let policy_id = CompactionPolicyId::new(peritus_codec::sha256(POLICY_LABEL));
    let policy = CompactionPolicy::new(policy_id, false);
    let output = bind_context_content(
        summary.as_bytes().to_vec(),
        peritus_codec::sha256(summary.as_bytes()),
        limits,
    )
    .map_err(|_| ControlError::InvalidInput)?;
    let range = SourceRange::new(source_id, source.digest(), 0, source.bytes())
        .map_err(|_| ControlError::InvalidInput)?;
    let proposal = CompactionProposal::new(
        output_id,
        policy_id,
        output,
        token_estimate(summary.len()),
        2,
        0,
        vec![range],
    )
    .map_err(|_| ControlError::InvalidInput)?;
    validate_compaction(&graph, &plan, &proposal, policy, limits)
        .map(|_| ())
        .map_err(|_| ControlError::InvalidInput)
}

fn context_id(
    label: &[u8],
    invocation: &[u8; 16],
    index: usize,
) -> Result<ContextNodeId, ControlError> {
    let mut bytes = POLICY_LABEL.to_vec();
    bytes.extend_from_slice(label);
    bytes.extend_from_slice(invocation);
    bytes.extend_from_slice(&index.to_be_bytes());
    let digest = peritus_codec::sha256(&bytes);
    let mut id = [0; 16];
    id.copy_from_slice(&digest.as_bytes()[..16]);
    id[0] |= 1;
    ContextNodeId::new(id).map_err(|_| ControlError::InvalidInput)
}

const fn token_estimate(bytes: usize) -> u64 {
    let value = bytes.div_ceil(4) as u64;
    if value == 0 { 1 } else { value }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().fold(String::with_capacity(bytes.len() * 2), |mut text, byte| {
        let _ = write!(text, "{byte:02x}");
        text
    })
}
