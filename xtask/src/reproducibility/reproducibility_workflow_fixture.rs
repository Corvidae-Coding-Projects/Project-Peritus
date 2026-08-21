pub(super) fn canonical_ci() -> String {
    include_str!("../../../.github/workflows/ci.yml").to_owned()
}
