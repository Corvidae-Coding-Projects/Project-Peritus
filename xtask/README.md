# Peritus xtask

`xtask` is the dependency-light engineering-policy executable for the Peritus workspace. It owns
no product or authority decisions. It validates the Cargo dependency graph, package ownership and
verification-class metadata, source layout and size budgets, trusted-Verus construct placement,
ordinary-Rust formal API boundaries, toolchain pins, lockfile policy, and immutable GitHub
Actions references. The reproducibility check also locks the reviewed GitHub Team repository-
ruleset activation template; live GitHub enforcement is verified separately through the documented
API runbook because an offline repository process cannot attest to external state.

`format-check` runs Rustfmt once per workspace package in deterministic package-name order. This
preserves the complete workspace formatting gate without exceeding the Windows process command-line
limit as the repository grows.

The checks return stable error categories and actionable diagnostics. CI and `just check` invoke
`cargo run --locked --package xtask -- all` directly so a repository-controlled Cargo alias cannot
swallow bootstrap evidence. Once the root Cargo configuration has passed policy, `cargo xtask all`
is the equivalent developer convenience interface.
