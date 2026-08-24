//! Runtime-neutral C2/C3 Windows conformance facts.

/// Observable Windows backend properties consumed by crate-local/A2 adapters.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "independent cross-platform conformance facts remain explicit"
)]
pub struct ConformanceFacts {
    /// Unsupported requests perform no preparation.
    pub unsupported_no_prepare: bool,
    /// Preparation remains bound to exact C2 identities.
    pub exact_binding: bool,
    /// Filesystem projection remains deny dominant.
    pub deny_dominant: bool,
    /// Helper protocol is fixed, bounded, and checksummed.
    pub helper_protocol_exact: bool,
    /// Cancellation remains C2-owned and observed once.
    pub cancellation_owned: bool,
    /// Release requires complete native cleanup.
    pub teardown_complete: bool,
}

impl ConformanceFacts {
    /// Reports whether all common Windows conformance facts hold.
    #[must_use]
    pub const fn complete(self) -> bool {
        self.unsupported_no_prepare
            && self.exact_binding
            && self.deny_dominant
            && self.helper_protocol_exact
            && self.cancellation_owned
            && self.teardown_complete
    }
}
