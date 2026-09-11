//! Explicit user-selected reasoning effort, independent of a provider's model identifier.

/// Requested effort. Provider/model support is checked separately, never silently coerced.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProductModelEffort {
    /// Preserve Peritus's existing policy: high when reasoning controls are available.
    Default,
    /// Minimal reasoning.
    Minimal,
    /// Low reasoning.
    Low,
    /// Medium reasoning.
    Medium,
    /// High reasoning.
    High,
    /// Extra-high reasoning on routes that support it.
    XHigh,
    /// Maximum reasoning on routes that support it.
    Max,
    /// Ultra reasoning on routes that support it.
    Ultra,
}

impl ProductModelEffort {
    /// Ordered choices for an interactive control; not a model-support claim.
    pub const ALL: [Self; 8] = [
        Self::Default,
        Self::Minimal,
        Self::Low,
        Self::Medium,
        Self::High,
        Self::XHigh,
        Self::Max,
        Self::Ultra,
    ];

    /// Stable command and display spelling.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Minimal => "minimal",
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::XHigh => "xhigh",
            Self::Max => "max",
            Self::Ultra => "ultra",
        }
    }

    /// Parses an exact supported spelling, without guessing or substituting.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|effort| effort.label() == value)
    }

    /// Stable persistence and wire allocation.
    #[must_use]
    pub const fn tag(self) -> u16 {
        match self {
            Self::Default => 0,
            Self::Minimal => 1,
            Self::Low => 2,
            Self::Medium => 3,
            Self::High => 4,
            Self::XHigh => 5,
            Self::Max => 6,
            Self::Ultra => 7,
        }
    }

    /// Decodes an allocated value; unknown values are not defaults.
    #[must_use]
    pub fn from_tag(tag: u16) -> Option<Self> {
        Self::ALL.into_iter().find(|effort| effort.tag() == tag)
    }
}
