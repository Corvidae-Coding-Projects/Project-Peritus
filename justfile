default: check

fmt:
    cargo fmt --all -- --check

build:
    cargo build --workspace --all-targets --all-features --locked

test:
    cargo test --workspace --all-targets --all-features --locked

doc-test:
    cargo test --doc --workspace --all-features --locked

clippy:
    cargo clippy --workspace --all-targets --all-features --locked -- -D warnings

docs:
    RUSTDOCFLAGS="-D warnings" cargo doc --workspace --all-features --no-deps --locked

architecture:
    cargo run --locked --package xtask -- architecture-check

source-layout:
    cargo run --locked --package xtask -- source-layout-check

reproducibility:
    cargo run --locked --package xtask -- reproducibility-check

trust:
    cargo run --locked --package xtask -- verify-trust

toolchain:
    cargo run --locked --package xtask -- toolchain-check

licenses:
    cargo deny --locked check bans licenses sources

deny:
    cargo deny --locked check

verus-verify:
    cargo verus verify --workspace --locked --check-toolchain

verus-build:
    cargo verus build --workspace --release --locked --check-toolchain

check: fmt build test doc-test clippy docs
    cargo run --locked --package xtask -- all

gate-a: check deny toolchain verus-verify verus-build
