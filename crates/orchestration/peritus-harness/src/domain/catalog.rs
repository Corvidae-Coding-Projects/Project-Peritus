//! Closed component and protection catalogs with compiled policy.

use crate::domain::{AuthoritySet, HarnessDomainError, HarnessDomainErrorKind};

/// Complete schema-v1 component catalog.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(u8)]
pub enum ComponentKind {
    /// Base instruction fragment.
    BaseInstructionFragment = 1,
    /// System instruction fragment.
    SystemInstructionFragment = 2,
    /// Role definition.
    RoleDefinition = 3,
    /// Role prompt.
    RolePrompt = 4,
    /// Tool descriptor.
    ToolDescriptor = 5,
    /// Tool input/output schema.
    ToolSchema = 6,
    /// Tool implementation artifact.
    ToolImplementation = 7,
    /// Tool exposure policy.
    ToolExposurePolicy = 8,
    /// Middleware definition.
    Middleware = 9,
    /// Context transform.
    ContextTransform = 10,
    /// Skill bundle.
    SkillBundle = 11,
    /// Reference bundle.
    ReferenceBundle = 12,
    /// Sub-agent definition.
    SubAgentDefinition = 13,
    /// Collaboration definition.
    CollaborationDefinition = 14,
    /// Memory schema.
    MemorySchema = 15,
    /// Memory selector.
    MemorySelector = 16,
    /// Memory ranking policy.
    MemoryRankingPolicy = 17,
    /// Memory retention policy.
    MemoryRetentionPolicy = 18,
    /// Memory injection policy.
    MemoryInjectionPolicy = 19,
    /// Gate definition.
    GateDefinition = 20,
    /// Gate result parser.
    GateParser = 21,
    /// Orchestration policy.
    OrchestrationPolicy = 22,
    /// Termination policy.
    TerminationPolicy = 23,
    /// Provider capability declaration.
    ProviderCapability = 24,
    /// Provider profile.
    ProviderProfile = 25,
    /// Observability policy.
    ObservabilityPolicy = 26,
    /// Redaction policy.
    RedactionPolicy = 27,
    /// Analysis policy.
    AnalysisPolicy = 28,
    /// Evolution strategy.
    EvolutionStrategy = 29,
    /// Metric definition.
    MetricDefinition = 30,
}

impl ComponentKind {
    /// Every component kind in canonical schema-tag order.
    pub const ALL: [Self; 30] = [
        Self::BaseInstructionFragment,
        Self::SystemInstructionFragment,
        Self::RoleDefinition,
        Self::RolePrompt,
        Self::ToolDescriptor,
        Self::ToolSchema,
        Self::ToolImplementation,
        Self::ToolExposurePolicy,
        Self::Middleware,
        Self::ContextTransform,
        Self::SkillBundle,
        Self::ReferenceBundle,
        Self::SubAgentDefinition,
        Self::CollaborationDefinition,
        Self::MemorySchema,
        Self::MemorySelector,
        Self::MemoryRankingPolicy,
        Self::MemoryRetentionPolicy,
        Self::MemoryInjectionPolicy,
        Self::GateDefinition,
        Self::GateParser,
        Self::OrchestrationPolicy,
        Self::TerminationPolicy,
        Self::ProviderCapability,
        Self::ProviderProfile,
        Self::ObservabilityPolicy,
        Self::RedactionPolicy,
        Self::AnalysisPolicy,
        Self::EvolutionStrategy,
        Self::MetricDefinition,
    ];

    /// Returns the immutable schema-v1 numeric tag.
    #[must_use]
    pub const fn tag(self) -> u8 {
        self as u8
    }

    pub(crate) fn from_tag(tag: u8) -> Result<Self, HarnessDomainError> {
        Self::ALL.into_iter().find(|kind| kind.tag() == tag).ok_or_else(|| {
            HarnessDomainError::detail(
                HarnessDomainErrorKind::InvalidCanonicalEncoding,
                "unknown component-kind tag",
            )
        })
    }

    /// Returns the protection class fixed by compiled policy.
    #[must_use]
    pub const fn protection_class(self) -> ProtectionClass {
        match self {
            Self::ToolExposurePolicy | Self::RedactionPolicy => ProtectionClass::SecurityRoot,
            Self::GateDefinition | Self::GateParser => ProtectionClass::HumanAuthority,
            Self::AnalysisPolicy | Self::MetricDefinition => ProtectionClass::SealedEvaluator,
            Self::ContextTransform
            | Self::MemoryInjectionPolicy
            | Self::ProviderCapability
            | Self::ProviderProfile => ProtectionClass::TrustBoundary,
            Self::EvolutionStrategy => ProtectionClass::ProductionPromotion,
            _ => ProtectionClass::Evolvable,
        }
    }

    /// Returns the compiled maximum descriptive authority for this component kind.
    #[must_use]
    pub const fn authority_ceiling(self) -> AuthoritySet {
        let bits = match self {
            Self::ToolImplementation | Self::Middleware => 0b00_0011_1111,
            Self::SkillBundle => 0b00_0111_1111,
            Self::GateDefinition => 0b00_1100_0011,
            Self::OrchestrationPolicy => 0b11_1100_0011,
            Self::ProviderProfile => 0b00_0011_0001,
            Self::AnalysisPolicy => 0b01_0000_0011,
            Self::EvolutionStrategy => 0b11_0000_0011,
            Self::ToolDescriptor
            | Self::ToolSchema
            | Self::ToolExposurePolicy
            | Self::ReferenceBundle
            | Self::MemorySchema
            | Self::MemorySelector
            | Self::MemoryRankingPolicy
            | Self::MemoryRetentionPolicy
            | Self::GateParser
            | Self::ProviderCapability
            | Self::ObservabilityPolicy
            | Self::RedactionPolicy
            | Self::MetricDefinition => 0b00_0000_0011,
            _ => 0b00_0000_0001,
        };
        AuthoritySet::from_known_bits(bits)
    }

    pub(crate) const fn accepts_protected_dependency(self, class: ProtectionClass) -> bool {
        match class {
            ProtectionClass::Evolvable => true,
            ProtectionClass::SecurityRoot => matches!(
                self,
                Self::ToolImplementation
                    | Self::Middleware
                    | Self::OrchestrationPolicy
                    | Self::ProviderProfile
            ),
            ProtectionClass::HumanAuthority => matches!(
                self,
                Self::OrchestrationPolicy | Self::TerminationPolicy | Self::GateDefinition
            ),
            ProtectionClass::SealedEvaluator => {
                matches!(self, Self::AnalysisPolicy | Self::EvolutionStrategy)
            }
            ProtectionClass::TrustBoundary => matches!(
                self,
                Self::ToolImplementation
                    | Self::Middleware
                    | Self::OrchestrationPolicy
                    | Self::ProviderProfile
            ),
            ProtectionClass::ProductionPromotion => {
                matches!(self, Self::EvolutionStrategy | Self::OrchestrationPolicy)
            }
        }
    }
}

/// Compiled protection of a controlled harness asset.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(u8)]
pub enum ProtectionClass {
    /// May change through a checked successor revision.
    Evolvable = 1,
    /// Root of security policy.
    SecurityRoot = 2,
    /// Human decision boundary.
    HumanAuthority = 3,
    /// Sealed evaluation definition.
    SealedEvaluator = 4,
    /// Trust-boundary definition.
    TrustBoundary = 5,
    /// Production-promotion definition.
    ProductionPromotion = 6,
}

impl ProtectionClass {
    /// Every protection class in canonical schema-tag order.
    pub const ALL: [Self; 6] = [
        Self::Evolvable,
        Self::SecurityRoot,
        Self::HumanAuthority,
        Self::SealedEvaluator,
        Self::TrustBoundary,
        Self::ProductionPromotion,
    ];

    /// Returns the immutable schema-v1 tag.
    #[must_use]
    pub const fn tag(self) -> u8 {
        self as u8
    }

    pub(crate) fn from_tag(tag: u8) -> Result<Self, HarnessDomainError> {
        Self::ALL.into_iter().find(|class| class.tag() == tag).ok_or_else(|| {
            HarnessDomainError::detail(
                HarnessDomainErrorKind::InvalidCanonicalEncoding,
                "unknown protection-class tag",
            )
        })
    }

    /// Returns whether this class is immutable across successors.
    #[must_use]
    pub const fn is_protected(self) -> bool {
        !matches!(self, Self::Evolvable)
    }
}
