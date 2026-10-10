//! Executable completeness predicates proved against compact mathematical projections.

#[doc = " Complete scalar projection of seven-domain plan compilation."]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CompilationFacts {
    #[doc = " Requested isolation\'s stable ordinal."]
    pub isolation_ordinal: u8,
    #[doc = " Operation class\'s stable ordinal."]
    pub operation_class_ordinal: u8,
    #[doc = " Filesystem requirements presented to the compiler."]
    pub filesystem_requested: usize,
    #[doc = " Filesystem requirements admitted by contract evaluation."]
    pub filesystem_admitted: usize,
    #[doc = " Process requirements presented to the compiler."]
    pub process_requested: usize,
    #[doc = " Process requirements admitted by contract evaluation."]
    pub process_admitted: usize,
    #[doc = " Environment names presented to the compiler."]
    pub environment_requested: usize,
    #[doc = " Environment names admitted by contract evaluation."]
    pub environment_admitted: usize,
    #[doc = " Network targets presented to the compiler."]
    pub network_requested: usize,
    #[doc = " Network targets admitted by contract evaluation."]
    pub network_admitted: usize,
    #[doc = " Secret deliveries presented to the compiler."]
    pub secrets_requested: usize,
    #[doc = " Secret deliveries admitted by contract evaluation."]
    pub secrets_admitted: usize,
    #[doc = " Resource requirements presented to the compiler."]
    pub resources_requested: usize,
    #[doc = " Resource requirements admitted by contract evaluation."]
    pub resources_admitted: usize,
    #[doc = " Terminal requirements presented to the compiler."]
    pub terminal_requested: usize,
    #[doc = " Terminal requirements admitted by contract evaluation."]
    pub terminal_admitted: usize,
}
#[doc = " Checks that every domain and the operation-class binding succeeded."]
#[must_use]
pub const fn compilation_complete(facts: CompilationFacts) -> bool {
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
#[doc = " Complete projection of backend admission facts."]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BackendFacts {
    #[doc = " Exact enforcement-feature bits required by the checked plan."]
    pub required_feature_bits: u64,
    #[doc = " Exact enforcement-feature bits supported by the backend descriptor."]
    pub supported_feature_bits: u64,
    #[doc = " Admission profile\'s stable ordinal."]
    pub profile_ordinal: u8,
    #[doc = " Backend kind\'s stable ordinal."]
    pub backend_kind_ordinal: u8,
}
#[doc = " Checks all backend admission facts without omitting a dimension."]
#[must_use]
pub const fn backend_complete(facts: BackendFacts) -> bool {
    facts.required_feature_bits & !facts.supported_feature_bits == 0
        && !(facts.profile_ordinal == 0 && facts.backend_kind_ordinal == 1)
}
#[doc = " Checks a non-wrapping resource charge against its exact limit."]
#[must_use]
pub const fn resource_charge_allowed(used: u64, charged: u64, limit: u64) -> bool {
    match used.checked_add(charged) {
        Some(total) => total <= limit,
        None => false,
    }
}
#[doc = " Checks the closed lifecycle edge relation encoded by stable phase ordinals."]
#[must_use]
pub const fn lifecycle_edge_allowed(current: u8, next: u8) -> bool {
    (current == 0 && next == 1)
        || (current == 1 && next == 2)
        || ((current == 1 || current == 2) && next == 3)
        || ((current == 2 || current == 3) && next == 4)
        || (current == 1 && next == 4)
        || (current == 4 && next == 5)
}
