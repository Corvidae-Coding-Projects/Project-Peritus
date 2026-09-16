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
    /// Logical view of the exact source node.
    pub closed spec fn spec_source_id(&self) -> ContextNodeId { self.source_id }

    /// Logical view of the frozen role-policy context class.
    pub closed spec fn spec_context_class(&self) -> ContextClass { self.context_class }

    /// Logical view of the provider-neutral message role.
    pub closed spec fn spec_message_role(&self) -> MessageRole { self.message_role }

    /// Logical view of unchanged source provenance.
    pub closed spec fn spec_provenance(&self) -> Provenance { self.provenance }

    /// Logical view of unchanged source authority.
    pub closed spec fn spec_authority(&self) -> AuthorityClass { self.authority }

    /// Logical view of unchanged source trust.
    pub closed spec fn spec_trust(&self) -> TrustClass { self.trust }

    /// Logical view of the verified source digest.
    pub closed spec fn spec_digest(&self) -> Sha256Digest { self.digest }

    /// Logical view of the unchanged semantic content kind.
    pub closed spec fn spec_content_kind(&self) -> ContentKind { self.content_kind }

    /// Logical view of exact source content.
    pub closed spec fn spec_content(&self) -> Seq<u8> { self.content@ }

    /// Whether this segment preserves every model-facing field of one exact source node.
    pub open spec fn spec_matches_node(&self, node: &crate::ContextNode) -> bool {
        &&& self.spec_source_id() == node.spec_id()
        &&& self.spec_context_class() == node.spec_context_class()
        &&& self.spec_message_role() == spec_role_for_authority(node.spec_authority())
        &&& self.spec_provenance() == node.spec_provenance()
        &&& self.spec_authority() == node.spec_authority()
        &&& self.spec_trust() == node.spec_trust()
        &&& self.spec_digest() == node.spec_digest()
        &&& self.spec_content_kind() == node.spec_content_kind()
        &&& self.spec_content() == node.spec_content_bytes()
    }

    fn from_node(node: &crate::ContextNode) -> (result: Self)
        ensures result.spec_matches_node(node),
    {
        let content_bytes = node.content().bytes();
        let content_len = content_bytes.len();
        let mut content = Vec::with_capacity(content_len);
        let mut content_index = 0;
        while content_index < content_len
            invariant
                content_index <= content_len,
                content_len == content_bytes@.len(),
                content@ == content_bytes@.subrange(0, content_index as int),
            decreases content_len - content_index,
        {
            content.push(content_bytes[content_index]);
            content_index += 1;
        }
        proof {
            assert(content_bytes@.subrange(0, content_bytes@.len() as int) =~= content_bytes@);
            assert(content@ == content_bytes@);
            assert(content_bytes@ == node.spec_content_bytes());
        }
        let result = Self {
            source_id: node.id(),
            context_class: node.context_class(),
            message_role: role_for_authority(node.authority()),
            provenance: node.provenance(),
            authority: node.authority(),
            trust: node.trust(),
            digest: node.digest(),
            content_kind: node.content_kind(),
            content,
        };
        proof {
            reveal(RenderSegment::spec_source_id);
            reveal(RenderSegment::spec_context_class);
            reveal(RenderSegment::spec_message_role);
            reveal(RenderSegment::spec_provenance);
            reveal(RenderSegment::spec_authority);
            reveal(RenderSegment::spec_trust);
            reveal(RenderSegment::spec_digest);
            reveal(RenderSegment::spec_content_kind);
            reveal(RenderSegment::spec_content);
        }
        result
    }

    /// Returns the exact source node.
    #[must_use]
    pub const fn source_id(&self) -> (result: ContextNodeId)
        ensures result == self.spec_source_id(),
    {
        self.source_id
    }
    /// Returns the frozen role-policy context class.
    #[must_use]
    pub const fn context_class(&self) -> (result: ContextClass)
        ensures result == self.spec_context_class(),
    {
        self.context_class
    }
    /// Returns the provider-neutral message role.
    #[must_use]
    pub const fn message_role(&self) -> (result: MessageRole)
        ensures result == self.spec_message_role(),
    {
        self.message_role
    }
    /// Returns the unchanged source provenance.
    #[must_use]
    pub const fn provenance(&self) -> (result: Provenance)
        ensures result == self.spec_provenance(),
    {
        self.provenance
    }
    /// Returns the unchanged source authority.
    #[must_use]
    pub const fn authority(&self) -> (result: AuthorityClass)
        ensures result == self.spec_authority(),
    {
        self.authority
    }
    /// Returns the unchanged source trust.
    #[must_use]
    pub const fn trust(&self) -> (result: TrustClass)
        ensures result == self.spec_trust(),
    {
        self.trust
    }
    /// Returns the unchanged verified content digest.
    #[must_use]
    pub const fn digest(&self) -> (result: Sha256Digest)
        ensures result == self.spec_digest(),
    {
        self.digest
    }
    /// Returns the unchanged semantic content kind.
    #[must_use]
    pub const fn content_kind(&self) -> (result: ContentKind)
        ensures result == self.spec_content_kind(),
    {
        self.content_kind
    }
    /// Borrows exact bounded source content.
    #[must_use]
    pub const fn content(&self) -> (result: &[u8])
        ensures result@ == self.spec_content(),
    {
        self.content.as_slice()
    }
}

/// Exact prefix correspondence between selected entries and rendered segments.
pub open spec fn spec_segments_match_selected(
    graph: &ContextGraph,
    selected: Seq<crate::SelectedContext>,
    segments: Seq<RenderSegment>,
) -> bool
    decreases segments.len(),
{
    if segments.len() == 0 {
        true
    } else if segments.len() > selected.len() {
        false
    } else {
        let index = segments.len() - 1;
        &&& spec_segments_match_selected(graph, selected, segments.drop_last())
        &&& match graph.spec_node_index(selected[index as int].spec_node_id()) {
            Some(node_index) => node_index < graph.spec_nodes().len()
                && segments[index as int]
                    .spec_matches_node(&graph.spec_nodes()[node_index as int]),
            None => false,
        }
    }
}

/// Complete provider-neutral rendering plan with exact selection accounting.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RenderPlan {
    segments: Vec<RenderSegment>,
    accounting: TokenAccounting,
    presentation: PresentationProfile,
}

impl RenderPlan {
    /// Logical view of segments in deterministic precedence order.
    pub closed spec fn spec_segments(&self) -> Seq<RenderSegment> { self.segments@ }

    /// Logical view of exact selected token accounting.
    pub closed spec fn spec_accounting(&self) -> TokenAccounting { self.accounting }

    /// Logical view of the frozen presentation policy.
    pub closed spec fn spec_presentation(&self) -> PresentationProfile { self.presentation }

    /// Exact correspondence between a selection plan and all rendered source fields.
    pub open spec fn spec_corresponds(&self, graph: &ContextGraph, plan: &ContextPlan) -> bool {
        &&& self.spec_segments().len() == plan.spec_selected().len()
        &&& self.spec_accounting() == plan.spec_accounting()
        &&& self.spec_presentation()
            == plan.spec_role_profile().spec_context().spec_presentation()
        &&& spec_segments_match_selected(
            graph,
            plan.spec_selected(),
            self.spec_segments(),
        )
    }

    /// Borrows separate segments in deterministic precedence order.
    #[must_use]
    pub const fn segments(&self) -> (result: &[RenderSegment])
        ensures result@ == self.spec_segments(),
    {
        self.segments.as_slice()
    }
    /// Returns exact selected token accounting.
    #[must_use]
    pub const fn accounting(&self) -> (result: TokenAccounting)
        ensures result == self.spec_accounting(),
    {
        self.accounting
    }
    /// Returns the frozen role presentation profile.
    #[must_use]
    pub const fn presentation(&self) -> (result: PresentationProfile)
        ensures result == self.spec_presentation(),
    {
        self.presentation
    }
}

/// Builds separate typed segments without provider encoding or text concatenation.
///
/// # Errors
///
/// Returns [`ContextErrorKind::PlanNodeMissing`] if the plan and graph do not correspond.
pub fn build_render_plan(
    graph: &ContextGraph,
    plan: &ContextPlan,
) -> (result: Result<RenderPlan, ContextError>)
    ensures match result {
        Ok(render) => render.spec_corresponds(graph, plan),
        Err(_) => true,
    },
{
    let selected_nodes = plan.selected();
    let selected_len = selected_nodes.len();
    let graph_nodes = graph.nodes();
    let mut segments: Vec<RenderSegment> = Vec::with_capacity(selected_len);
    let mut index = 0;
    while index < selected_len
        invariant
            index <= selected_len,
            selected_len == selected_nodes@.len(),
            selected_nodes@ == plan.spec_selected(),
            graph_nodes@ == graph.spec_nodes(),
            segments@.len() == index,
            spec_segments_match_selected(graph, selected_nodes@, segments@),
        decreases selected_len - index,
    {
        let selected = selected_nodes[index];
        let Some(node_index) = graph.index_of(selected.node_id()) else {
            return Err(ContextError::node(
                ContextErrorKind::PlanNodeMissing,
                selected.node_id(),
            ));
        };
        let node = &graph_nodes[node_index];
        if !node.visibility().contains(plan.role_profile().actor_role())
            || !plan.role_profile().context().visible().contains(node.context_class())
        {
            return Err(ContextError::node(
                ContextErrorKind::PlanNodeMissing,
                selected.node_id(),
            ));
        }
        let segment = RenderSegment::from_node(node);
        proof {
            assert(graph.spec_node_index(selected.spec_node_id()) == Some(node_index as nat));
            assert(segment.spec_matches_node(&graph.spec_nodes()[node_index as int]));
        }
        let ghost prior_segments = segments@;
        segments.push(segment);
        proof {
            assert(prior_segments.len() == index);
            assert(segments@ == prior_segments.push(segment));
            assert(segments@.drop_last() == prior_segments);
            assert(segments@.last() == segment);
            assert(selected_nodes@[prior_segments.len() as int] == selected);
            reveal_with_fuel(spec_segments_match_selected, 1);
            assert(spec_segments_match_selected(graph, selected_nodes@, segments@));
        }
        index += 1;
    }
    let render = RenderPlan {
        segments,
        accounting: plan.accounting(),
        presentation: plan.role_profile().context().presentation(),
    };
    proof {
        reveal(RenderPlan::spec_segments);
        reveal(RenderPlan::spec_accounting);
        reveal(RenderPlan::spec_presentation);
        reveal(RenderPlan::spec_corresponds);
        assert(render.spec_segments() == segments@);
        assert(render.spec_segments().len() == plan.spec_selected().len());
        assert(render.spec_accounting() == plan.spec_accounting());
        assert(render.spec_presentation()
            == plan.spec_role_profile().spec_context().spec_presentation());
        assert(spec_segments_match_selected(
            graph,
            plan.spec_selected(),
            render.spec_segments(),
        ));
        assert(render.spec_corresponds(graph, plan));
    }
    Ok(render)
}

/// Exact model-facing role for each C6 authority class.
pub open spec fn spec_role_for_authority(authority: AuthorityClass) -> MessageRole {
    match authority {
        AuthorityClass::SystemPolicy => MessageRole::System,
        AuthorityClass::ApplicationPolicy | AuthorityClass::AcceptanceSpecification => {
            MessageRole::Application
        }
        AuthorityClass::UserInstruction => MessageRole::User,
        AuthorityClass::NonAuthoritative => MessageRole::Evidence,
    }
}

const fn role_for_authority(authority: AuthorityClass) -> (result: MessageRole)
    ensures result == spec_role_for_authority(authority),
{
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
