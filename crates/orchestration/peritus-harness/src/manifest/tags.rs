//! Strict textual manifest tags mapped to the closed domain catalogs.

use serde::Deserialize;

use crate::domain::{Authority, ComponentKind, ProtectionClass};

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum RawComponentKind {
    BaseInstructionFragment,
    SystemInstructionFragment,
    RoleDefinition,
    RolePrompt,
    ToolDescriptor,
    ToolSchema,
    ToolImplementation,
    ToolExposurePolicy,
    Middleware,
    ContextTransform,
    SkillBundle,
    ReferenceBundle,
    SubAgentDefinition,
    CollaborationDefinition,
    MemorySchema,
    MemorySelector,
    MemoryRankingPolicy,
    MemoryRetentionPolicy,
    MemoryInjectionPolicy,
    GateDefinition,
    GateParser,
    OrchestrationPolicy,
    TerminationPolicy,
    ProviderCapability,
    ProviderProfile,
    ObservabilityPolicy,
    RedactionPolicy,
    AnalysisPolicy,
    EvolutionStrategy,
    MetricDefinition,
}

impl From<RawComponentKind> for ComponentKind {
    fn from(value: RawComponentKind) -> Self {
        match value {
            RawComponentKind::BaseInstructionFragment => Self::BaseInstructionFragment,
            RawComponentKind::SystemInstructionFragment => Self::SystemInstructionFragment,
            RawComponentKind::RoleDefinition => Self::RoleDefinition,
            RawComponentKind::RolePrompt => Self::RolePrompt,
            RawComponentKind::ToolDescriptor => Self::ToolDescriptor,
            RawComponentKind::ToolSchema => Self::ToolSchema,
            RawComponentKind::ToolImplementation => Self::ToolImplementation,
            RawComponentKind::ToolExposurePolicy => Self::ToolExposurePolicy,
            RawComponentKind::Middleware => Self::Middleware,
            RawComponentKind::ContextTransform => Self::ContextTransform,
            RawComponentKind::SkillBundle => Self::SkillBundle,
            RawComponentKind::ReferenceBundle => Self::ReferenceBundle,
            RawComponentKind::SubAgentDefinition => Self::SubAgentDefinition,
            RawComponentKind::CollaborationDefinition => Self::CollaborationDefinition,
            RawComponentKind::MemorySchema => Self::MemorySchema,
            RawComponentKind::MemorySelector => Self::MemorySelector,
            RawComponentKind::MemoryRankingPolicy => Self::MemoryRankingPolicy,
            RawComponentKind::MemoryRetentionPolicy => Self::MemoryRetentionPolicy,
            RawComponentKind::MemoryInjectionPolicy => Self::MemoryInjectionPolicy,
            RawComponentKind::GateDefinition => Self::GateDefinition,
            RawComponentKind::GateParser => Self::GateParser,
            RawComponentKind::OrchestrationPolicy => Self::OrchestrationPolicy,
            RawComponentKind::TerminationPolicy => Self::TerminationPolicy,
            RawComponentKind::ProviderCapability => Self::ProviderCapability,
            RawComponentKind::ProviderProfile => Self::ProviderProfile,
            RawComponentKind::ObservabilityPolicy => Self::ObservabilityPolicy,
            RawComponentKind::RedactionPolicy => Self::RedactionPolicy,
            RawComponentKind::AnalysisPolicy => Self::AnalysisPolicy,
            RawComponentKind::EvolutionStrategy => Self::EvolutionStrategy,
            RawComponentKind::MetricDefinition => Self::MetricDefinition,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum RawProtectionClass {
    Evolvable,
    SecurityRoot,
    HumanAuthority,
    SealedEvaluator,
    TrustBoundary,
    ProductionPromotion,
}

impl From<RawProtectionClass> for ProtectionClass {
    fn from(value: RawProtectionClass) -> Self {
        match value {
            RawProtectionClass::Evolvable => Self::Evolvable,
            RawProtectionClass::SecurityRoot => Self::SecurityRoot,
            RawProtectionClass::HumanAuthority => Self::HumanAuthority,
            RawProtectionClass::SealedEvaluator => Self::SealedEvaluator,
            RawProtectionClass::TrustBoundary => Self::TrustBoundary,
            RawProtectionClass::ProductionPromotion => Self::ProductionPromotion,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum RawAuthority {
    ContextRead,
    WorkspaceRead,
    WorkspaceMutation,
    ProcessExecution,
    NetworkAccess,
    SecretReference,
    ApprovalRequest,
    AcceptanceObservation,
    EvaluationInput,
    PromotionProposal,
}

impl From<RawAuthority> for Authority {
    fn from(value: RawAuthority) -> Self {
        match value {
            RawAuthority::ContextRead => Self::ContextRead,
            RawAuthority::WorkspaceRead => Self::WorkspaceRead,
            RawAuthority::WorkspaceMutation => Self::WorkspaceMutation,
            RawAuthority::ProcessExecution => Self::ProcessExecution,
            RawAuthority::NetworkAccess => Self::NetworkAccess,
            RawAuthority::SecretReference => Self::SecretReference,
            RawAuthority::ApprovalRequest => Self::ApprovalRequest,
            RawAuthority::AcceptanceObservation => Self::AcceptanceObservation,
            RawAuthority::EvaluationInput => Self::EvaluationInput,
            RawAuthority::PromotionProposal => Self::PromotionProposal,
        }
    }
}
