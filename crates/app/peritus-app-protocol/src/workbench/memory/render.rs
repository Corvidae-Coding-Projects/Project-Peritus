//! Scope-filtered deterministic future-request guidance rendering.

use super::{
    MAX_WORKBENCH_GUIDANCE_PAGE, MAX_WORKBENCH_GUIDANCE_RECORDS,
    MAX_WORKBENCH_GUIDANCE_RENDER_BYTES, WorkbenchGuidanceIdentity, WorkbenchGuidanceRecord,
    WorkbenchGuidanceScope, WorkbenchGuidanceSource, WorkbenchGuidanceTombstone, capacity, invalid,
};
use crate::{AppProtocolError, ConversationId};
use peritus_types::{Sha256Digest, WorkspaceId};

/// Exact bounded project-guidance block ready for a checked future context node.
#[derive(Clone, Eq, PartialEq)]
pub struct WorkbenchGuidanceRender {
    dependency_revision: u64,
    identities: Vec<WorkbenchGuidanceIdentity>,
    text: String,
    digest: Sha256Digest,
}

impl WorkbenchGuidanceRender {
    /// Returns the workspace dependency revision from which this block was built.
    #[must_use]
    pub const fn dependency_revision(&self) -> u64 {
        self.dependency_revision
    }

    /// Borrows the exact ordered included identities.
    #[must_use]
    pub fn identities(&self) -> &[WorkbenchGuidanceIdentity] {
        &self.identities
    }

    /// Borrows exact deterministic provider-neutral text.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Returns SHA-256 over exact rendered UTF-8 bytes.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.digest
    }
}

impl std::fmt::Debug for WorkbenchGuidanceRender {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("WorkbenchGuidanceRender")
            .field("dependency_revision", &self.dependency_revision)
            .field("identities", &self.identities)
            .field("bytes", &self.text.len())
            .field("digest", &self.digest)
            .finish()
    }
}

/// Filters scope/tombstones before deterministically rendering exact future-request guidance.
///
/// # Errors
/// Rejects duplicate/mismatched state, non-dominating tombstone conflicts, or context-size overflow.
pub fn render_guidance_for_request(
    workspace: WorkspaceId,
    conversation: ConversationId,
    dependency_revision: u64,
    records: &[WorkbenchGuidanceRecord],
    tombstones: &[WorkbenchGuidanceTombstone],
) -> Result<WorkbenchGuidanceRender, AppProtocolError> {
    if records.len().saturating_add(tombstones.len()) > MAX_WORKBENCH_GUIDANCE_RECORDS {
        return Err(capacity());
    }
    let mut tombstones_by_id = std::collections::BTreeMap::new();
    for tombstone in tombstones {
        if tombstone.identity().workspace() != workspace
            || tombstone.dependency_revision() > dependency_revision
            || tombstones_by_id.insert(tombstone.identity().id(), tombstone).is_some()
        {
            return Err(invalid());
        }
    }
    let mut seen = std::collections::BTreeSet::new();
    let mut selected = Vec::new();
    for record in records {
        if record.identity().workspace() != workspace
            || record.version().dependency() > dependency_revision
            || !seen.insert(record.identity().id())
        {
            return Err(invalid());
        }
        if let Some(tombstone) = tombstones_by_id.get(&record.identity().id()) {
            if tombstone.prior().revision() < record.version().record()
                || tombstone.prior().digest() != record.content().digest()
            {
                return Err(invalid());
            }
            continue;
        }
        if scope_matches(record.content().scope(), conversation) {
            selected.push(record);
        }
    }
    selected.sort_by(|left, right| {
        right
            .pinned()
            .cmp(&left.pinned())
            .then_with(|| left.identity().id().cmp(&right.identity().id()))
    });
    if selected.len() > MAX_WORKBENCH_GUIDANCE_PAGE {
        return Err(capacity());
    }
    let mut text = String::new();
    text.push_str("PERITUS_PROJECT_GUIDANCE_V1\nworkspace=");
    push_hex(&mut text, workspace.as_bytes());
    text.push_str("\nconversation=");
    push_hex(&mut text, conversation.as_bytes());
    text.push_str("\ndependency_revision=");
    text.push_str(&dependency_revision.to_string());
    text.push_str(
        "\nnotice=user-approved project guidance; not authority; no cross-project reuse\nrecords=",
    );
    text.push_str(&selected.len().to_string());
    text.push('\n');
    let mut identities = Vec::with_capacity(selected.len());
    for record in selected {
        identities.push(record.identity());
        render_record(&mut text, record);
        if text.len() > MAX_WORKBENCH_GUIDANCE_RENDER_BYTES {
            return Err(capacity());
        }
    }
    text.push_str("END_PERITUS_PROJECT_GUIDANCE_V1\n");
    if text.len() > MAX_WORKBENCH_GUIDANCE_RENDER_BYTES {
        return Err(capacity());
    }
    let digest = peritus_codec::sha256(text.as_bytes());
    Ok(WorkbenchGuidanceRender { dependency_revision, identities, text, digest })
}

pub(super) fn scope_matches(scope: WorkbenchGuidanceScope, conversation: ConversationId) -> bool {
    matches!(scope, WorkbenchGuidanceScope::Project)
        || matches!(scope, WorkbenchGuidanceScope::Conversation(id) if id == conversation)
}

fn render_record(output: &mut String, record: &WorkbenchGuidanceRecord) {
    output.push_str("record id=");
    push_hex(output, record.identity().id().as_bytes());
    output.push_str(" revision=");
    output.push_str(&record.version().record().to_string());
    output.push_str(" scope=");
    match record.content().scope() {
        WorkbenchGuidanceScope::Project => output.push_str("project"),
        WorkbenchGuidanceScope::Conversation(id) => {
            output.push_str("conversation:");
            push_hex(output, id.as_bytes());
        }
    }
    output.push_str(" pinned=");
    output.push_str(if record.pinned() { "true" } else { "false" });
    output.push_str(" source=");
    match record.content().source() {
        WorkbenchGuidanceSource::UserAuthored => output.push_str("user-authored"),
        WorkbenchGuidanceSource::AcceptedPublicReply { operation, invocation, digest } => {
            output.push_str("accepted-public-reply:");
            push_hex(output, operation.as_bytes());
            output.push(':');
            push_hex(output, invocation.as_bytes());
            output.push(':');
            push_hex(output, digest.as_bytes());
        }
    }
    output.push_str(" validated_by=");
    push_hex(output, record.last_validation().operation().as_bytes());
    output.push_str(" content_sha256=");
    push_hex(output, record.content().digest().as_bytes());
    output.push_str(" utf8_bytes=");
    output.push_str(&record.content().text().as_str().len().to_string());
    output.push_str("\ntext=\"");
    push_escaped(output, record.content().text().as_str());
    output.push_str("\"\n");
}

fn push_hex(output: &mut String, bytes: &[u8]) {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    for byte in bytes {
        output.push(char::from(HEX[usize::from(byte >> 4)]));
        output.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
}

fn push_escaped(output: &mut String, text: &str) {
    for character in text.chars() {
        match character {
            '\\' => output.push_str("\\\\"),
            '"' => output.push_str("\\\""),
            '\n' => output.push_str("\\n"),
            '\t' => output.push_str("\\t"),
            other => output.push(other),
        }
    }
}
