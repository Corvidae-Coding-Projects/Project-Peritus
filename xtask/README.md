# Peritus xtask

`xtask` is the dependency-light engineering-policy executable for the Peritus workspace. It owns
no product or authority decisions. It validates the Cargo dependency graph, package ownership and
verification-class metadata, source layout and size budgets, trusted-Verus construct placement,
toolchain pins, lockfile policy, and immutable GitHub Actions references.

The checks return stable error categories and actionable diagnostics. CI and `just check` invoke
`cargo run --locked --package xtask -- all` directly so a repository-controlled Cargo alias cannot
swallow bootstrap evidence. Once the root Cargo configuration has passed policy, `cargo xtask all`
is the equivalent developer convenience interface.
