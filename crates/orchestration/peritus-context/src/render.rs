//! Provider-neutral typed render segments preserving every authority boundary.

use crate::{
    AuthorityClass, ContentKind, ContextError, ContextErrorKind, ContextGraph, ContextNodeId,
    ContextPlan, Provenance, TokenAccounting, TrustClass,
};
use peritus_role::{ContextClass, PresentationProfile};
use peritus_types::Sha256Digest;
use vstd::prelude::*;

verus! {

/// Provider-neutral semantic message role.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum MessageRole {
    /// System policy only.
    System,
    /// Application policy and immutable specification only.
    Application,
    /// Active user instructions only.
    User,
    /// Delimited non-authoritative evidence.
    Evidence,
}

/// One separately delimited model-facing context segment.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RenderSegment {
    source_id: ContextNodeId,
    context_class: ContextClass,
    message_role: MessageRole,
    provenance: Provenance,
    authority: AuthorityClass,
    trust: TrustClass,
    digest: Sha256Digest,
    content_kind: ContentKind,
    content: Vec<u8>,
}

impl RenderSegment {
    /// Returns the exact source node.
    #[must_use]
    pub const fn source_id(&self) -> ContextNodeId { self.source_id }
    /// Returns the frozen role-policy context class.
    #[must_use]
    pub const fn context_class(&self) -> ContextClass { self.context_class }
    /// Returns the provider-neutral message role.
    #[must_use]
    pub const fn message_role(&self) -> MessageRole { self.message_role }
    /// Returns the unchanged source provenance.
    #[must_use]
    pub const fn provenance(&self) -> Provenance { self.provenance }
    /// Returns the unchanged source authority.
    #[must_use]
    pub const fn authority(&self) -> AuthorityClass { self.authority }
    /// Returns the unchanged source trust.
    #[must_use]
    pub const fn trust(&self) -> TrustClass { self.trust }
    /// Returns the unchanged verified content digest.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest { self.digest }
    /// Returns the unchanged semantic content kind.
    #[must_use]
    pub const fn content_kind(&self) -> ContentKind { self.content_kind }
    /// Borrows exact bounded source content.
    #[must_use]
    pub const fn content(&self) -> &[u8] { self.content.as_slice() }
}

/// Complete provider-neutral rendering plan with exact selection accounting.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RenderPlan {
    segments: Vec<RenderSegment>,
    accounting: TokenAccounting,
    presentation: PresentationProfile,
}

impl RenderPlan {
    /// Borrows separate segments in deterministic precedence order.
    #[must_use]
    pub const fn segments(&self) -> &[RenderSegment] { self.segments.as_slice() }
    /// Returns exact selected token accounting.
    #[must_use]
    pub const fn accounting(&self) -> TokenAccounting { self.accounting }
    /// Returns the frozen role presentation profile.
    #[must_use]
    pub const fn presentation(&self) -> PresentationProfile { self.presentation }
}

/// Builds separate typed segments without provider encoding or text concatenation.
///
/// # Errors
///
/// Returns [`ContextErrorKind::PlanNodeMissing`] if the plan and graph do not correspond.
pub fn build_render_plan(
    graph: &ContextGraph,
    plan: &ContextPlan,
) -> Result<RenderPlan, ContextError> {
    let selected_nodes = plan.selected();
    let selected_len = selected_nodes.len();
    let mut segments = Vec::with_capacity(selected_len);
    let mut index = 0;
    while index < selected_len
        invariant
            index <= selected_len,
            selected_len == selected_nodes@.len(),
        decreases selected_len - index,
    {
        let selected = selected_nodes[index];
        let Some(node) = graph.node(selected.node_id()) else {
            return Err(ContextError::node(
                ContextErrorKind::PlanNodeMissing,
                selected.node_id(),
            ));
        };
        if !node.visibility().contains(plan.role_profile().actor_role())
            || !plan.role_profile().context().visible().contains(node.context_class())
        {
            return Err(ContextError::node(
                ContextErrorKind::PlanNodeMissing,
                selected.node_id(),
            ));
        }
        let content_bytes = node.content().bytes();
        let content_len = content_bytes.len();
        let mut content = Vec::with_capacity(content_len);
        let mut content_index = 0;
        while content_index < content_len
            invariant
                content_index <= content_len,
                content_len == content_bytes@.len(),
            decreases content_len - content_index,
        {
            content.push(content_bytes[content_index]);
            content_index += 1;
        }
        segments.push(RenderSegment {
            source_id: node.id(),
            context_class: node.context_class(),
            message_role: role_for_authority(node.authority()),
            provenance: node.provenance(),
            authority: node.authority(),
            trust: node.trust(),
            digest: node.digest(),
            content_kind: node.content_kind(),
            content,
        });
        index += 1;
    }
    Ok(RenderPlan {
        segments,
        accounting: plan.accounting(),
        presentation: plan.role_profile().context().presentation(),
    })
}

const fn role_for_authority(authority: AuthorityClass) -> MessageRole {
    match authority {
        AuthorityClass::SystemPolicy => MessageRole::System,
        AuthorityClass::ApplicationPolicy | AuthorityClass::AcceptanceSpecification => {
            MessageRole::Application
        }
        AuthorityClass::UserInstruction => MessageRole::User,
        AuthorityClass::NonAuthoritative => MessageRole::Evidence,
    }
}

} // verus!
