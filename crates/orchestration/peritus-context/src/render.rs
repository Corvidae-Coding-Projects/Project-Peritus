//! Provider-neutral typed render segments preserving every authority boundary.

use crate::{
    AuthorityClass, ContentKind, ContextError, ContextErrorKind, ContextGraph, ContextNodeId,
    ContextPlan, Provenance, TokenAccounting, TrustClass,
};
use peritus_role::{ContextClass, PresentationProfile};
use peritus_types::Sha256Digest;

#[doc = " Provider-neutral semantic message role."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum MessageRole {
    #[doc = " System policy only."]
    System,
    #[doc = " Application policy and immutable specification only."]
    Application,
    #[doc = " Active user instructions only."]
    User,
    #[doc = " Delimited non-authoritative evidence."]
    Evidence,
}
#[doc = " One separately delimited model-facing context segment."]
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
    fn from_node(node: &crate::ContextNode) -> Self {
        let content_bytes = node.content().bytes();
        let content_len = content_bytes.len();
        let mut content = Vec::with_capacity(content_len);
        let mut content_index = 0;
        while content_index < content_len {
            content.push(content_bytes[content_index]);
            content_index += 1;
        }

        Self {
            source_id: node.id(),
            context_class: node.context_class(),
            message_role: role_for_authority(node.authority()),
            provenance: node.provenance(),
            authority: node.authority(),
            trust: node.trust(),
            digest: node.digest(),
            content_kind: node.content_kind(),
            content,
        }
    }
    #[doc = " Returns the exact source node."]
    #[must_use]
    pub const fn source_id(&self) -> ContextNodeId {
        self.source_id
    }
    #[doc = " Returns the frozen role-policy context class."]
    #[must_use]
    pub const fn context_class(&self) -> ContextClass {
        self.context_class
    }
    #[doc = " Returns the provider-neutral message role."]
    #[must_use]
    pub const fn message_role(&self) -> MessageRole {
        self.message_role
    }
    #[doc = " Returns the unchanged source provenance."]
    #[must_use]
    pub const fn provenance(&self) -> Provenance {
        self.provenance
    }
    #[doc = " Returns the unchanged source authority."]
    #[must_use]
    pub const fn authority(&self) -> AuthorityClass {
        self.authority
    }
    #[doc = " Returns the unchanged source trust."]
    #[must_use]
    pub const fn trust(&self) -> TrustClass {
        self.trust
    }
    #[doc = " Returns the unchanged verified content digest."]
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.digest
    }
    #[doc = " Returns the unchanged semantic content kind."]
    #[must_use]
    pub const fn content_kind(&self) -> ContentKind {
        self.content_kind
    }
    #[doc = " Borrows exact bounded source content."]
    #[must_use]
    pub const fn content(&self) -> &[u8] {
        self.content.as_slice()
    }
}
#[doc = " Complete provider-neutral rendering plan with exact selection accounting."]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RenderPlan {
    segments: Vec<RenderSegment>,
    accounting: TokenAccounting,
    presentation: PresentationProfile,
}
impl RenderPlan {
    #[doc = " Borrows separate segments in deterministic precedence order."]
    #[must_use]
    pub const fn segments(&self) -> &[RenderSegment] {
        self.segments.as_slice()
    }
    #[doc = " Returns exact selected token accounting."]
    #[must_use]
    pub const fn accounting(&self) -> TokenAccounting {
        self.accounting
    }
    #[doc = " Returns the frozen role presentation profile."]
    #[must_use]
    pub const fn presentation(&self) -> PresentationProfile {
        self.presentation
    }
}
#[doc = " Builds separate typed segments without provider encoding or text concatenation."]
#[doc = ""]
#[doc = " # Errors"]
#[doc = ""]
#[doc = " Returns [`ContextErrorKind::PlanNodeMissing`] if the plan and graph do not correspond."]
pub fn build_render_plan(
    graph: &ContextGraph,
    plan: &ContextPlan,
) -> Result<RenderPlan, ContextError> {
    let selected_nodes = plan.selected();
    let selected_len = selected_nodes.len();
    let graph_nodes = graph.nodes();
    let mut segments: Vec<RenderSegment> = Vec::with_capacity(selected_len);
    let mut index = 0;
    while index < selected_len {
        let selected = selected_nodes[index];
        let Some(node_index) = graph.index_of(selected.node_id()) else {
            return Err(ContextError::node(ContextErrorKind::PlanNodeMissing, selected.node_id()));
        };
        let node = &graph_nodes[node_index];
        if !node.visibility().contains(plan.role_profile().actor_role())
            || !plan.role_profile().context().visible().contains(node.context_class())
        {
            return Err(ContextError::node(ContextErrorKind::PlanNodeMissing, selected.node_id()));
        }
        let segment = RenderSegment::from_node(node);
        segments.push(segment);
        index += 1;
    }
    let render = RenderPlan {
        segments,
        accounting: plan.accounting(),
        presentation: plan.role_profile().context().presentation(),
    };
    Ok(render)
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
