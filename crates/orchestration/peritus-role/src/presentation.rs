//! Provider-neutral role presentation preferences.

use vstd::prelude::*;

verus! {

/// Model-facing organization style selected by the agent loop.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum PresentationStyle {
    /// Implementation-focused context with current workspace state.
    Implementation,
    /// Evidence-focused fresh review context.
    AdversarialReview,
    /// Finding-focused repair context.
    FindingResolution,
    /// Frozen-definition evaluation context.
    IsolatedEvaluation,
    /// Harness-analysis and candidate-evolution context.
    HarnessEvolution,
    /// Minimal read-only service context for non-agent roles.
    Restricted,
}

/// Provider-neutral presentation facts. They do not select a model or grant authority.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PresentationProfile {
    style: PresentationStyle,
    separate_provenance_segments: bool,
    include_selection_reasons: bool,
    include_token_accounting: bool,
}

impl PresentationProfile {
    /// Exact stored style.
    pub closed spec fn spec_style(&self) -> PresentationStyle { self.style }
    /// Exact stored separate provenance segments.
    pub closed spec fn spec_separate_provenance_segments(&self) -> bool { self.separate_provenance_segments }
    /// Exact stored include selection reasons.
    pub closed spec fn spec_include_selection_reasons(&self) -> bool { self.include_selection_reasons }
    /// Exact stored include token accounting.
    pub closed spec fn spec_include_token_accounting(&self) -> bool { self.include_token_accounting }
    /// Exact provider-neutral profile for a chosen style.
    pub open spec fn spec_for_style(&self, style: PresentationStyle) -> bool {
        self.spec_style() == style && self.spec_separate_provenance_segments()
            && self.spec_include_selection_reasons() && self.spec_include_token_accounting()
    }

    pub(crate) const fn new(style: PresentationStyle) -> (profile: Self)
        ensures profile.spec_for_style(style),
    {
        Self {
            style,
            separate_provenance_segments: true,
            include_selection_reasons: true,
            include_token_accounting: true,
        }
    }

    /// Returns the organization style.
    #[must_use]
    pub const fn style(&self) -> (value: PresentationStyle)
        ensures value == self.spec_style(),
    { self.style }

    /// Whether provenance boundaries must remain separate model segments.
    #[must_use]
    pub const fn separate_provenance_segments(&self) -> (value: bool)
        ensures value == self.spec_separate_provenance_segments(),
    {
        self.separate_provenance_segments
    }

    /// Whether omission and ranking reasons are retained for inspection.
    #[must_use]
    pub const fn include_selection_reasons(&self) -> (value: bool)
        ensures value == self.spec_include_selection_reasons(),
    { self.include_selection_reasons }

    /// Whether exact token accounting is attached to the render plan.
    #[must_use]
    pub const fn include_token_accounting(&self) -> (value: bool)
        ensures value == self.spec_include_token_accounting(),
    { self.include_token_accounting }
}

} // verus!
