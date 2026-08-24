//! Fixed sandbox inputs shared by the executable cases.

use super::{SandboxConformanceFixture, SandboxDomain, SandboxFeature, SandboxScenario};

pub(super) const ALL_DOMAINS: &[SandboxDomain] = &[
    SandboxDomain::Filesystem,
    SandboxDomain::Process,
    SandboxDomain::Environment,
    SandboxDomain::Network,
    SandboxDomain::Secret,
    SandboxDomain::Resource,
    SandboxDomain::Terminal,
];
pub(super) const SECRET_CANARY: &[u8] = b"peritus-secret-canary-never-observed";
pub(super) const ORDER_A: &[SandboxFeature] = &[
    SandboxFeature::Pty,
    SandboxFeature::FilesystemRead,
    SandboxFeature::TreeContainment,
    SandboxFeature::OutputBytes,
];
pub(super) const ORDER_B: &[SandboxFeature] = &[
    SandboxFeature::OutputBytes,
    SandboxFeature::TreeContainment,
    SandboxFeature::FilesystemRead,
    SandboxFeature::Pty,
];
pub(super) const CANONICAL_FEATURES: &[SandboxFeature] = &[
    SandboxFeature::FilesystemRead,
    SandboxFeature::OutputBytes,
    SandboxFeature::Pty,
    SandboxFeature::TreeContainment,
];
pub(super) const BACKEND_FEATURES: &[SandboxFeature] = CANONICAL_FEATURES;

pub(super) const fn fixture(scenario: SandboxScenario) -> SandboxConformanceFixture {
    let resource_requested = match scenario {
        SandboxScenario::ResourceAtLimit => 8,
        SandboxScenario::ResourceOverLimit => 9,
        _ => 1,
    };
    SandboxConformanceFixture::new(
        scenario,
        "/workspace/src/lib.rs",
        "PERITUS_SECRET",
        "api.example.invalid",
        443,
        "secret://conformance/api-token",
        SECRET_CANARY,
        8,
        resource_requested,
    )
}
