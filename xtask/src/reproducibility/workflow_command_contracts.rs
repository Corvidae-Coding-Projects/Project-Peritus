pub(super) const PRE_CARGO_AUTHORITY: &str = "6ca5f56d2ab12e93f155d684b33f4a86c2f877b8";

pub(super) const WORKSPACE_TEST_ARGS: &[&str] = &[
    "test",
    "--workspace",
    "--all-targets",
    "--all-features",
    "--locked",
    "--",
    "--test-threads=1",
];
