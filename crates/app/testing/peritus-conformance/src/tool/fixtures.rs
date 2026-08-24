//! Fixed C4 tool fixtures and independently varied authority dimensions.

use super::{ToolAuthorizationDrift, ToolConformanceFixture, ToolScenario};

pub(super) const AUTHORITY_DRIFTS: [ToolAuthorizationDrift; 12] = [
    ToolAuthorizationDrift::Action,
    ToolAuthorizationDrift::Descriptor,
    ToolAuthorizationDrift::Arguments,
    ToolAuthorizationDrift::OperationClass,
    ToolAuthorizationDrift::ActorRole,
    ToolAuthorizationDrift::Resource,
    ToolAuthorizationDrift::Capability,
    ToolAuthorizationDrift::Budget,
    ToolAuthorizationDrift::Lease,
    ToolAuthorizationDrift::Dispatch,
    ToolAuthorizationDrift::Revision,
    ToolAuthorizationDrift::AuthorityTime,
];

pub(super) const fn fixture(scenario: ToolScenario) -> ToolConformanceFixture {
    ToolConformanceFixture::new(
        scenario,
        "fs.read",
        br#"{"path":"src/lib.rs","max_bytes":4096}"#,
        4_096,
        30_000,
    )
}
