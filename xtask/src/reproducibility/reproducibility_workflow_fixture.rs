pub(super) fn canonical_ci() -> String {
    include_str!("../../../.github/workflows/ci.yml").to_owned()
}

pub(super) fn canonical_governance() -> String {
    include_str!("../../../.github/workflows/formal-governance.yml").to_owned()
}
