//! Checked context nodes and canonical role/dependency metadata.

mod metadata;
mod visibility;

pub use metadata::ContextNodeMetadata;
pub use visibility::RoleVisibility;

use crate::{
    AuthorityClass, ContentKind, ContextContent, ContextError, ContextLimits, ContextNodeId,
    Provenance, TrustClass,
};
use peritus_role::ContextClass;
use peritus_types::Sha256Digest;

#[doc = " Whether a node is a required root, a preferred dependency, or optional."]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum RequirementMode {
    #[doc = " The node and its complete closure must be selected."]
    Required,
    #[doc = " The node is required whenever a selected root depends on it."]
    DependencyRequired,
    #[doc = " The node is eligible for atomic optional admission."]
    Optional,
}
impl RequirementMode {
    pub(crate) const fn precedence(self) -> u8 {
        match self {
            Self::Required => 3,
            Self::DependencyRequired => 2,
            Self::Optional => 1,
        }
    }
}
#[doc = " One immutable, content-bound node in a canonical context DAG."]
#[derive(Debug, Eq, PartialEq)]
pub struct ContextNode {
    metadata: ContextNodeMetadata,
    content: ContextContent,
}
impl ContextNode {
    pub(crate) fn clone_sequence(nodes: &[Self]) -> Vec<Self> {
        let mut result = Vec::with_capacity(nodes.len());
        let mut index = 0;
        while index < nodes.len() {
            result.push(nodes[index].clone());
            index += 1;
        }
        result
    }
    #[doc = " Joins checked metadata with digest-verified bounded content."]
    #[must_use]
    pub const fn new(metadata: ContextNodeMetadata, content: ContextContent) -> Self {
        Self { metadata, content }
    }
    #[doc = " Returns the stable node identifier."]
    #[must_use]
    pub const fn id(&self) -> ContextNodeId {
        self.metadata.id()
    }
    #[doc = " Returns the verified content digest."]
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.content.digest()
    }
    #[doc = " Returns the immutable bounded content."]
    #[must_use]
    pub const fn content(&self) -> &ContextContent {
        &self.content
    }
    #[doc = " Returns the source provenance."]
    #[must_use]
    pub const fn provenance(&self) -> Provenance {
        self.metadata.provenance_value()
    }
    #[doc = " Returns the authority class."]
    #[must_use]
    pub const fn authority(&self) -> AuthorityClass {
        self.metadata.authority_value()
    }
    #[doc = " Returns the trust class."]
    #[must_use]
    pub const fn trust(&self) -> TrustClass {
        self.metadata.trust_value()
    }
    #[doc = " Returns the role-policy context class."]
    #[must_use]
    pub const fn context_class(&self) -> ContextClass {
        self.metadata.context_class_value()
    }
    #[doc = " Returns the semantic content kind."]
    #[must_use]
    pub const fn content_kind(&self) -> ContentKind {
        self.metadata.content_kind_value()
    }
    #[doc = " Returns the caller-supplied positive token estimate."]
    #[must_use]
    pub const fn token_estimate(&self) -> u64 {
        self.metadata.token_estimate_value()
    }
    #[doc = " Returns the caller-supplied positive logical recency sequence."]
    #[must_use]
    pub const fn recency_sequence(&self) -> u64 {
        self.metadata.recency_sequence_value()
    }
    #[doc = " Returns the requirement mode."]
    #[must_use]
    pub const fn requirement(&self) -> RequirementMode {
        self.metadata.requirement_value()
    }
    #[doc = " Returns the explicit optional-ranking priority."]
    #[must_use]
    pub const fn priority(&self) -> u16 {
        self.metadata.priority_value()
    }
    #[doc = " Returns the explicit role visibility set."]
    #[must_use]
    pub const fn visibility(&self) -> &RoleVisibility {
        self.metadata.visibility_value()
    }
    #[doc = " Returns canonical direct dependency identities."]
    #[must_use]
    pub const fn dependencies(&self) -> &[ContextNodeId] {
        self.metadata.dependencies_value()
    }
    pub(crate) fn replace_dependencies(
        &self,
        dependencies: Vec<ContextNodeId>,
        limits: ContextLimits,
    ) -> Result<Self, ContextError> {
        let metadata = ContextNodeMetadata::new(
            self.id(),
            self.provenance(),
            self.authority(),
            self.trust(),
            self.context_class(),
            self.content_kind(),
            self.token_estimate(),
            self.recency_sequence(),
            self.requirement(),
            self.priority(),
            self.visibility().clone(),
            dependencies,
            limits,
        )?;
        let content = self.content.clone();
        let result = Self::new(metadata, content);
        Ok(result)
    }
}
impl Clone for ContextNode {
    fn clone(&self) -> Self {
        let metadata = self.metadata.clone();
        let content = self.content.clone();
        Self { metadata, content }
    }
}
