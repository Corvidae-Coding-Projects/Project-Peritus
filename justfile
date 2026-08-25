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
    cargo verus verify --package peritus-agent --package peritus-approval --package peritus-artifact-store --package peritus-budget --package peritus-codec --package peritus-context --package peritus-evidence --package peritus-gates --package peritus-git --package peritus-journal --package peritus-kernel --package peritus-leases --package peritus-memory --package peritus-migrations --package peritus-model-protocol --package peritus-network --package peritus-patch --package peritus-policy --package peritus-process --package peritus-projection --package peritus-protocol --package peritus-provider-anthropic --package peritus-provider-compatible --package peritus-provider-core --package peritus-provider-google --package peritus-provider-openai --package peritus-quality-policy --package peritus-review --package peritus-role --package peritus-sandbox --package peritus-sandbox-linux --package peritus-sandbox-macos --package peritus-sandbox-windows --package peritus-secrets --package peritus-spec --package peritus-telemetry --package peritus-tool-protocol --package peritus-tool-router --package peritus-tools-fs --package peritus-tools-git --package peritus-tools-quality --package peritus-tools-shell --package peritus-trace --package peritus-types --package peritus-workspace --all-features --locked --check-toolchain --fwd-verus-args-to roots -- --no-cheating --rlimit 20

verus-build:
    cargo verus build --workspace --all-features --release --locked --check-toolchain --fwd-verus-args-to roots -- --rlimit 20
    cargo verus build --package peritus-agent --package peritus-approval --package peritus-artifact-store --package peritus-budget --package peritus-codec --package peritus-context --package peritus-evidence --package peritus-gates --package peritus-git --package peritus-journal --package peritus-kernel --package peritus-leases --package peritus-memory --package peritus-migrations --package peritus-model-protocol --package peritus-network --package peritus-patch --package peritus-policy --package peritus-process --package peritus-projection --package peritus-protocol --package peritus-provider-anthropic --package peritus-provider-compatible --package peritus-provider-core --package peritus-provider-google --package peritus-provider-openai --package peritus-quality-policy --package peritus-review --package peritus-role --package peritus-sandbox --package peritus-sandbox-linux --package peritus-sandbox-macos --package peritus-sandbox-windows --package peritus-secrets --package peritus-spec --package peritus-telemetry --package peritus-tool-protocol --package peritus-tool-router --package peritus-tools-fs --package peritus-tools-git --package peritus-tools-quality --package peritus-tools-shell --package peritus-trace --package peritus-types --package peritus-workspace --all-features --release --locked --check-toolchain --fwd-verus-args-to roots -- --no-cheating --rlimit 20

check: fmt build test doc-test clippy docs
    cargo run --locked --package xtask -- all

gate-a: check ordinary-api deny toolchain verus-verify verus-build
