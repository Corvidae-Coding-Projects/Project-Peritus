use vstd::prelude::*;

verus! {

/// Repository-relative location of the durable verification-actor registry.
#[must_use]
pub const fn actors_manifest_path() -> &'static str {
    "verification/actors.toml"
}

/// Repository-relative location of the trusted-construct inventory.
#[must_use]
pub const fn trust_manifest_path() -> &'static str {
    "verification/trust.toml"
}

/// Repository-relative location of the Verus coverage-exclusion inventory.
#[must_use]
pub const fn exclusions_manifest_path() -> &'static str {
    "verification/exclusions.toml"
}

/// Repository-relative location of the proof-obligation inventory.
#[must_use]
pub const fn obligations_manifest_path() -> &'static str {
    "verification/obligations.toml"
}

/// Repository-relative location of the formal-source change-review inventory.
#[must_use]
pub const fn proof_impact_manifest_path() -> &'static str {
    "verification/proof-impact.toml"
}

/// Every verification manifest in the policy boundary.
#[must_use]
pub const fn verification_manifest_paths() -> [&'static str; 5] {
    [
        actors_manifest_path(),
        trust_manifest_path(),
        exclusions_manifest_path(),
        obligations_manifest_path(),
        proof_impact_manifest_path(),
    ]
}

} // verus!
