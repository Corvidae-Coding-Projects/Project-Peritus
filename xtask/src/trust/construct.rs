#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Construct {
    Assume,
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
}

impl Construct {
    pub(super) const fn label(self) -> &'static str {
        match self {
            Self::Assume => "assume",
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
        }
    }
}
