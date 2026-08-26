//! Deterministic complete harness graph validation.

use crate::domain::graph_validation::{
    decode_features, decode_limits, encode_features, encode_limits, protected_inventory,
    resolve_requirements, topological_order, u64_len, validate_authority_closures,
    validate_graph_bounds, validate_order, validate_unique_declarations,
};
use crate::domain::{
    ArtifactRoot, AuthoritySet, CanonicalEncoder, CanonicalReader, ComponentDeclaration,
    ComponentId, FeatureTag, GraphDigest, HarnessDomainError, HarnessDomainErrorKind,
    HarnessLimitKind, HarnessLimits, ProtectionClass,
};

const GRAPH_DOMAIN: &[u8] = b"peritus-e1-checked-graph-v1\0";

/// Exact provider and platform feature inventory used for graph checking.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GraphEnvironment {
    pub(super) provider_features: Vec<FeatureTag>,
    pub(super) platform_features: Vec<FeatureTag>,
}

impl GraphEnvironment {
    /// Creates an environment from strict canonical feature sets.
    ///
    /// # Errors
    ///
    /// Rejects duplicate or descending tags.
    pub fn new(
        provider_features: Vec<FeatureTag>,
        platform_features: Vec<FeatureTag>,
    ) -> Result<Self, HarnessDomainError> {
        validate_order(&provider_features, "provider environment")?;
        validate_order(&platform_features, "platform environment")?;
        Ok(Self { provider_features, platform_features })
    }

    /// Borrows supported provider features.
    #[must_use]
    pub fn provider_features(&self) -> &[FeatureTag] {
        &self.provider_features
    }
    /// Borrows supported platform features.
    #[must_use]
    pub fn platform_features(&self) -> &[FeatureTag] {
        &self.platform_features
    }
}

/// One resolved dependency edge.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedEdge {
    pub(super) depender: ComponentId,
    pub(super) dependency: ComponentId,
}

impl ResolvedEdge {
    /// Returns the component declaring the edge.
    #[must_use]
    pub const fn depender(&self) -> &ComponentId {
        &self.depender
    }
    /// Returns the resolved dependency.
    #[must_use]
    pub const fn dependency(&self) -> &ComponentId {
        &self.dependency
    }
}

/// Canonical fingerprint of one immutable protected declaration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProtectedAsset {
    pub(super) component_id: ComponentId,
    pub(super) declaration_position: u64,
    pub(super) protection_class: ProtectionClass,
    pub(super) declaration_digest: peritus_types::Sha256Digest,
}

impl ProtectedAsset {
    /// Returns the protected component identity.
    #[must_use]
    pub const fn component_id(&self) -> &ComponentId {
        &self.component_id
    }
    /// Returns the committed zero-based manifest declaration position.
    #[must_use]
    pub const fn declaration_position(&self) -> u64 {
        self.declaration_position
    }
    /// Returns the compiled protection class.
    #[must_use]
    pub const fn protection_class(&self) -> ProtectionClass {
        self.protection_class
    }
    /// Returns the digest binding every declaration field and its ordered dependencies.
    #[must_use]
    pub const fn declaration_digest(&self) -> peritus_types::Sha256Digest {
        self.declaration_digest
    }
}

/// Immutable graph obtainable only through complete validation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CheckedHarnessGraph {
    declarations: Vec<ComponentDeclaration>,
    declaration_order: Vec<ComponentId>,
    edges: Vec<ResolvedEdge>,
    topological_order: Vec<ComponentId>,
    graph_digest: GraphDigest,
    aggregate_authority: AuthoritySet,
    provider_features: Vec<FeatureTag>,
    platform_features: Vec<FeatureTag>,
    protected_assets: Vec<ProtectedAsset>,
    artifact_roots: Vec<ArtifactRoot>,
    limits: HarnessLimits,
}

impl CheckedHarnessGraph {
    /// Runs all structural, compatibility, feature, authority, and protection checks.
    ///
    /// # Errors
    ///
    /// Returns a structured diagnostic for the first canonical validation failure.
    pub fn check(
        mut declarations: Vec<ComponentDeclaration>,
        environment: &GraphEnvironment,
        limits: HarnessLimits,
    ) -> Result<Self, HarnessDomainError> {
        validate_graph_bounds(&declarations, limits)?;
        let declaration_order: Vec<ComponentId> =
            declarations.iter().map(|item| item.id().clone()).collect();
        declarations.sort_by(|left, right| left.id().cmp(right.id()));
        validate_unique_declarations(&declarations)?;
        let (edges, provider_features, platform_features) =
            resolve_requirements(&declarations, environment)?;
        let topological_order = topological_order(&declarations)?;
        let aggregate_authority = validate_authority_closures(&declarations, &topological_order)?;
        let protected_assets = protected_inventory(&declarations, &declaration_order);
        let artifact_roots: Vec<ArtifactRoot> =
            declarations.iter().map(ArtifactRoot::from_declaration).collect();
        let mut graph = Self {
            declarations,
            declaration_order,
            edges,
            topological_order,
            graph_digest: GraphDigest::new(peritus_codec::sha256(&[])),
            aggregate_authority,
            provider_features,
            platform_features,
            protected_assets,
            artifact_roots,
            limits,
        };
        graph.graph_digest = GraphDigest::new(peritus_codec::sha256(&graph.canonical_bytes()));
        Ok(graph)
    }

    /// Reconstructs a graph from canonical bytes and reruns the complete checker.
    ///
    /// # Errors
    ///
    /// Rejects malformed, trailing, noncanonical, or semantically invalid bytes.
    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, HarnessDomainError> {
        let mut reader = CanonicalReader::new(bytes, GRAPH_DOMAIN)?;
        let limits = decode_limits(&mut reader)?;
        let provider_features = decode_features(&mut reader, limits)?;
        let platform_features = decode_features(&mut reader, limits)?;
        let declaration_count = reader.length()?;
        if u64_len(declaration_count) > limits.max_components() {
            return Err(HarnessDomainError::limit(
                HarnessDomainErrorKind::TooManyComponents,
                HarnessLimitKind::Components,
                limits.max_components(),
                u64_len(declaration_count),
            ));
        }
        let mut declarations_by_id = Vec::with_capacity(declaration_count);
        for _ in 0..declaration_count {
            declarations_by_id.push(ComponentDeclaration::decode(&mut reader, limits)?);
        }
        let order_count = reader.length()?;
        if order_count != declaration_count {
            return Err(HarnessDomainError::plain(
                HarnessDomainErrorKind::InvalidCanonicalEncoding,
            ));
        }
        let mut order = Vec::with_capacity(order_count);
        for _ in 0..order_count {
            order.push(ComponentId::new(reader.string()?)?);
        }
        reader.finish()?;
        let mut declarations = Vec::with_capacity(order.len());
        for id in order {
            let declaration = declarations_by_id
                .iter()
                .find(|candidate| candidate.id() == &id)
                .ok_or_else(|| {
                    HarnessDomainError::component(
                        HarnessDomainErrorKind::MissingDependency,
                        id.clone(),
                    )
                })?;
            declarations.push(declaration.clone());
        }
        let environment = GraphEnvironment::new(provider_features, platform_features)?;
        let graph = Self::check(declarations, &environment, limits)?;
        if graph.canonical_bytes() != bytes {
            return Err(HarnessDomainError::plain(
                HarnessDomainErrorKind::InvalidCanonicalEncoding,
            ));
        }
        Ok(graph)
    }

    /// Borrows declarations in canonical component-ID order.
    #[must_use]
    pub fn declarations(&self) -> &[ComponentDeclaration] {
        &self.declarations
    }
    /// Looks up a declaration by stable identity.
    #[must_use]
    pub fn declaration(&self, id: &ComponentId) -> Option<&ComponentDeclaration> {
        self.declarations
            .binary_search_by(|item| item.id().cmp(id))
            .ok()
            .map(|index| &self.declarations[index])
    }
    /// Borrows manifest declaration order, which is committed by graph identity.
    #[must_use]
    pub fn declaration_order(&self) -> &[ComponentId] {
        &self.declaration_order
    }
    /// Borrows resolved edges in canonical depender and declared-dependency order.
    #[must_use]
    pub fn resolved_edges(&self) -> &[ResolvedEdge] {
        &self.edges
    }
    /// Borrows deterministic dependency-first topological order.
    #[must_use]
    pub fn topological_order(&self) -> &[ComponentId] {
        &self.topological_order
    }
    /// Returns the complete graph digest.
    #[must_use]
    pub const fn graph_digest(&self) -> GraphDigest {
        self.graph_digest
    }
    /// Returns aggregate transitive descriptive authority.
    #[must_use]
    pub const fn aggregate_authority(&self) -> AuthoritySet {
        self.aggregate_authority
    }
    /// Borrows the union of provider requirements.
    #[must_use]
    pub fn required_provider_features(&self) -> &[FeatureTag] {
        &self.provider_features
    }
    /// Borrows the union of platform requirements.
    #[must_use]
    pub fn required_platform_features(&self) -> &[FeatureTag] {
        &self.platform_features
    }
    /// Borrows protected assets in committed declaration order.
    #[must_use]
    pub fn protected_assets(&self) -> &[ProtectedAsset] {
        &self.protected_assets
    }
    /// Borrows all component content and executable roots in component-ID order.
    #[must_use]
    pub fn artifact_roots(&self) -> &[ArtifactRoot] {
        &self.artifact_roots
    }
    /// Returns the exact limits used to validate the graph.
    #[must_use]
    pub const fn limits(&self) -> HarnessLimits {
        self.limits
    }

    /// Returns the deterministic schema-v1 graph encoding.
    #[must_use]
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut encoder = CanonicalEncoder::new(GRAPH_DOMAIN);
        encode_limits(&mut encoder, self.limits);
        encode_features(&mut encoder, &self.provider_features);
        encode_features(&mut encoder, &self.platform_features);
        encoder.len(self.declarations.len());
        for declaration in &self.declarations {
            declaration.encode(&mut encoder);
        }
        encoder.len(self.declaration_order.len());
        for id in &self.declaration_order {
            encoder.string(id.as_str());
        }
        encoder.into_bytes()
    }
}
