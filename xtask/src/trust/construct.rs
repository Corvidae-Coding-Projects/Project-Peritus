#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Construct {
    Assume,
    BuiltinAssume,
    Admit,
    Axiom,
    AssumeSpecification,
    External,
    ExternalBody,
    ExternalFunctionSpecification,
    ExternalTypeSpecification,
    ExternalTraitSpecification,
    ExternalTraitExtension,
    ExternalTraitPrivateBound,
    ExternalDerive,
    ExternalTraitBlanket,
    Trusted,
    AssumeTermination,
    ExecAllowsNoDecreases,
    ExecSpecUnverified,
    InlineAirStatement,
    AllowInlineAir,
    GhostAssumeNew,
    GhostAssumeNewFallback,
    TrackedAssumeNew,
    TrackedAssumeNewFallback,
    AssumeNew,
    AssumeNewFallback,
    ProhibitedTrustedImport,
}

impl Construct {
    const ALL: [Self; 26] = [
        Self::Assume,
        Self::BuiltinAssume,
        Self::Admit,
        Self::Axiom,
        Self::AssumeSpecification,
        Self::External,
        Self::ExternalBody,
        Self::ExternalFunctionSpecification,
        Self::ExternalTypeSpecification,
        Self::ExternalTraitSpecification,
        Self::ExternalTraitExtension,
        Self::ExternalTraitPrivateBound,
        Self::ExternalDerive,
        Self::ExternalTraitBlanket,
        Self::Trusted,
        Self::AssumeTermination,
        Self::ExecAllowsNoDecreases,
        Self::ExecSpecUnverified,
        Self::InlineAirStatement,
        Self::AllowInlineAir,
        Self::GhostAssumeNew,
        Self::GhostAssumeNewFallback,
        Self::TrackedAssumeNew,
        Self::TrackedAssumeNewFallback,
        Self::AssumeNew,
        Self::AssumeNewFallback,
    ];

    pub(super) fn is_known_label(label: &str) -> bool {
        Self::ALL.iter().any(|construct| construct.label() == label)
    }

    pub(super) const fn label(self) -> &'static str {
        match self {
            Self::Assume => "assume",
            Self::BuiltinAssume => "builtin::assume_",
            Self::Admit => "admit",
            Self::Axiom => "axiom",
            Self::AssumeSpecification => "assume_specification",
            Self::External => "external",
            Self::ExternalBody => "external_body",
            Self::ExternalFunctionSpecification => "external_fn_specification",
            Self::ExternalTypeSpecification => "external_type_specification",
            Self::ExternalTraitSpecification => "external_trait_specification",
            Self::ExternalTraitExtension => "external_trait_extension",
            Self::ExternalTraitPrivateBound => "external_trait_private_bound",
            Self::ExternalDerive => "external_derive",
            Self::ExternalTraitBlanket => "external_trait_blanket",
            Self::Trusted => "verus::trusted",
            Self::AssumeTermination => "verifier::assume_termination",
            Self::ExecAllowsNoDecreases => "verifier::exec_allows_no_decreases_clause",
            Self::ExecSpecUnverified => "exec_spec_unverified",
            Self::InlineAirStatement => "inline_air_stmt",
            Self::AllowInlineAir => concat!("allow", "-inline-air"),
            Self::GhostAssumeNew => "Ghost::assume_new",
            Self::GhostAssumeNewFallback => "Ghost::assume_new_fallback",
            Self::TrackedAssumeNew => "Tracked::assume_new",
            Self::TrackedAssumeNewFallback => "Tracked::assume_new_fallback",
            Self::AssumeNew => "*::assume_new",
            Self::AssumeNewFallback => "*::assume_new_fallback",
            Self::ProhibitedTrustedImport => "trusted-operation-import",
        }
    }

    pub(super) const fn is_prohibited_everywhere(self) -> bool {
        matches!(self, Self::ProhibitedTrustedImport)
    }
}
