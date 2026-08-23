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

ordinary-api:
    cargo run --locked --package xtask -- ordinary-api-check

toolchain:
    cargo run --locked --package xtask -- toolchain-check

licenses:
    cargo deny --locked check bans licenses sources

deny:
    cargo deny --locked check

verus-verify:
    cargo verus verify --workspace --all-features --locked --check-toolchain --fwd-verus-args-to roots -- --rlimit 20
    cargo verus verify --package peritus-approval --package peritus-artifact-store --package peritus-budget --package peritus-codec --package peritus-evidence --package peritus-journal --package peritus-kernel --package peritus-leases --package peritus-migrations --package peritus-policy --package peritus-projection --package peritus-protocol --package peritus-quality-policy --package peritus-spec --package peritus-types --all-features --locked --check-toolchain --fwd-verus-args-to roots -- --no-cheating --rlimit 20

verus-build:
    cargo verus build --workspace --all-features --release --locked --check-toolchain --fwd-verus-args-to roots -- --rlimit 20
    cargo verus build --package peritus-approval --package peritus-artifact-store --package peritus-budget --package peritus-codec --package peritus-evidence --package peritus-journal --package peritus-kernel --package peritus-leases --package peritus-migrations --package peritus-policy --package peritus-projection --package peritus-protocol --package peritus-quality-policy --package peritus-spec --package peritus-types --all-features --release --locked --check-toolchain --fwd-verus-args-to roots -- --no-cheating --rlimit 20

check: fmt build test doc-test clippy docs
    cargo run --locked --package xtask -- all

gate-a: check ordinary-api deny toolchain verus-verify verus-build
