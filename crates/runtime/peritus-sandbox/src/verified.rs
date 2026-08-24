//! Executable completeness predicates proved against compact mathematical projections.

use vstd::prelude::*;

verus! {

/// Complete scalar projection of seven-domain plan compilation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CompilationFacts {
    /// Requested isolation's stable ordinal.
    pub isolation_ordinal: u8,
    /// Operation class's stable ordinal.
    pub operation_class_ordinal: u8,
    /// Filesystem requirements presented to the compiler.
    pub filesystem_requested: usize,
    /// Filesystem requirements admitted by contract evaluation.
    pub filesystem_admitted: usize,
    /// Process requirements presented to the compiler.
    pub process_requested: usize,
    /// Process requirements admitted by contract evaluation.
    pub process_admitted: usize,
    /// Environment names presented to the compiler.
    pub environment_requested: usize,
    /// Environment names admitted by contract evaluation.
    pub environment_admitted: usize,
    /// Network targets presented to the compiler.
    pub network_requested: usize,
    /// Network targets admitted by contract evaluation.
    pub network_admitted: usize,
    /// Secret deliveries presented to the compiler.
    pub secrets_requested: usize,
    /// Secret deliveries admitted by contract evaluation.
    pub secrets_admitted: usize,
    /// Resource requirements presented to the compiler.
    pub resources_requested: usize,
    /// Resource requirements admitted by contract evaluation.
    pub resources_admitted: usize,
    /// Terminal requirements presented to the compiler.
    pub terminal_requested: usize,
    /// Terminal requirements admitted by contract evaluation.
    pub terminal_admitted: usize,
}

/// Mathematical completeness definition for plan compilation.
pub open spec fn compilation_complete_spec(facts: CompilationFacts) -> bool {
    ((facts.isolation_ordinal == 0 && facts.operation_class_ordinal == 0)
        || (facts.isolation_ordinal == 1 && facts.operation_class_ordinal == 1))
        && facts.filesystem_requested == facts.filesystem_admitted
        && facts.process_requested == facts.process_admitted
        && facts.environment_requested == facts.environment_admitted
        && facts.network_requested == facts.network_admitted
        && facts.secrets_requested == facts.secrets_admitted
        && facts.resources_requested == facts.resources_admitted
        && facts.terminal_requested == facts.terminal_admitted
}

/// Checks that every domain and the operation-class binding succeeded.
#[must_use]
pub const fn compilation_complete(facts: CompilationFacts) -> (complete: bool)
    ensures complete == compilation_complete_spec(facts),
{
    ((facts.isolation_ordinal == 0 && facts.operation_class_ordinal == 0)
        || (facts.isolation_ordinal == 1 && facts.operation_class_ordinal == 1))
        && facts.filesystem_requested == facts.filesystem_admitted
        && facts.process_requested == facts.process_admitted
        && facts.environment_requested == facts.environment_admitted
        && facts.network_requested == facts.network_admitted
        && facts.secrets_requested == facts.secrets_admitted
        && facts.resources_requested == facts.resources_admitted
        && facts.terminal_requested == facts.terminal_admitted
}

/// Complete projection of backend admission facts.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BackendFacts {
    /// Exact enforcement-feature bits required by the checked plan.
    pub required_feature_bits: u64,
    /// Exact enforcement-feature bits supported by the backend descriptor.
    pub supported_feature_bits: u64,
    /// Admission profile's stable ordinal.
    pub profile_ordinal: u8,
    /// Backend kind's stable ordinal.
    pub backend_kind_ordinal: u8,
}

/// Mathematical completeness definition for backend admission.
pub open spec fn backend_complete_spec(facts: BackendFacts) -> bool {
    facts.required_feature_bits & !facts.supported_feature_bits == 0
        && !(facts.profile_ordinal == 0 && facts.backend_kind_ordinal == 1)
}

/// Checks all backend admission facts without omitting a dimension.
#[must_use]
pub const fn backend_complete(facts: BackendFacts) -> (complete: bool)
    ensures complete == backend_complete_spec(facts),
{
    facts.required_feature_bits & !facts.supported_feature_bits == 0
        && !(facts.profile_ordinal == 0 && facts.backend_kind_ordinal == 1)
}

/// Checks a non-wrapping resource charge against its exact limit.
#[must_use]
pub const fn resource_charge_allowed(
    used: u64,
    charged: u64,
    limit: u64,
) -> (allowed: bool)
    ensures allowed == (used as int + charged as int <= limit as int),
{
    match used.checked_add(charged) {
        Some(total) => total <= limit,
        None => false,
    }
}

/// Checks the closed lifecycle edge relation encoded by stable phase ordinals.
#[must_use]
pub const fn lifecycle_edge_allowed(current: u8, next: u8) -> (allowed: bool)
    ensures allowed == (
        (current == 0 && next == 1)
            || (current == 1 && next == 2)
            || ((current == 1 || current == 2) && next == 3)
            || ((current == 2 || current == 3) && next == 4)
            || (current == 1 && next == 4)
            || (current == 4 && next == 5)
    ),
{
    (current == 0 && next == 1)
        || (current == 1 && next == 2)
        || ((current == 1 || current == 2) && next == 3)
        || ((current == 2 || current == 3) && next == 4)
        || (current == 1 && next == 4)
        || (current == 4 && next == 5)
}

} // verus!
