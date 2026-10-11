//! Provider-neutral role presentation preferences.

#[doc = " Model-facing organization style selected by the agent loop."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum PresentationStyle {
    #[doc = " Implementation-focused context with current workspace state."]
    Implementation,
    #[doc = " Evidence-focused fresh review context."]
    AdversarialReview,
    #[doc = " Finding-focused repair context."]
    FindingResolution,
    #[doc = " Frozen-definition evaluation context."]
    IsolatedEvaluation,
    #[doc = " Harness-analysis and candidate-evolution context."]
    HarnessEvolution,
    #[doc = " Minimal read-only service context for non-agent roles."]
    Restricted,
}
#[doc = " Provider-neutral presentation facts. They do not select a model or grant authority."]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PresentationProfile {
    style: PresentationStyle,
    separate_provenance_segments: bool,
    include_selection_reasons: bool,
    include_token_accounting: bool,
}
impl PresentationProfile {
    pub(crate) const fn new(style: PresentationStyle) -> Self {
        Self {
            style,
            separate_provenance_segments: true,
            include_selection_reasons: true,
            include_token_accounting: true,
        }
    }
    #[doc = " Returns the organization style."]
    #[must_use]
    pub const fn style(&self) -> PresentationStyle {
        self.style
    }
    #[doc = " Whether provenance boundaries must remain separate model segments."]
    #[must_use]
    pub const fn separate_provenance_segments(&self) -> bool {
        self.separate_provenance_segments
    }
    #[doc = " Whether omission and ranking reasons are retained for inspection."]
    #[must_use]
    pub const fn include_selection_reasons(&self) -> bool {
        self.include_selection_reasons
    }
    #[doc = " Whether exact token accounting is attached to the render plan."]
    #[must_use]
    pub const fn include_token_accounting(&self) -> bool {
        self.include_token_accounting
    }
}
