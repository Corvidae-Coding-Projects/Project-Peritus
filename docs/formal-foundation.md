# Formal foundation

A1 establishes the value-type and trust-accounting substrate used by later verified state
machines. It does not claim that any later lifecycle, policy, acceptance, or evolution invariant
already exists.

## Crate boundaries

`peritus-types` is verification class V. It owns only time-independent primitive values:

- 29 nominal 16-byte identifier types whose all-zero representation is rejected;
- one-based revision, event-sequence, and generation numbers with checked advancement;
- an exact 32-byte SHA-256 digest representation, without hashing or authenticity claims;
- capability names matching `[a-z][a-z0-9-]*(.[a-z][a-z0-9-]*)*` within 128 bytes, with a
  verified canonical ASCII-byte order;
- an exact revision tuple binding acceptance, harness, workspace, policy, and provider identity; and
- resource dimensions and nonnegative quantities with checked addition and subtraction.

Fields are private. Ordinary Rust callers use total constructors returning typed errors; they do
not need to satisfy hidden Verus `requires` clauses. The executable constructors and arithmetic
carry specifications proving the same postconditions used by verified callers. Serialization,
UUIDs, random generation, clocks, hashing, I/O, and authority decisions remain outside this crate.

`peritus-tcb` is verification class T and the sole source root that may eventually contain a
narrowly reviewed Verus trusted construct or external specification. Its A1 baseline contains
zero such constructs and no authority logic. Class T identifies the future boundary; it does not
make source trusted merely because it is located in the crate.

Both crates opt into Cargo-Verus verification and depend only on the pinned `vstd` revision.

## Verification manifests

The versioned files under `verification/` are authoritative inventories:

- `actors.toml` resolves stable owner/reviewer IDs to typed automation principals and roles through
  the raw-byte hash of `actor-provenance.json`; that retained record carries exact repository,
  issue creation, session, task, execution-mode, model, effort, and immutable locator evidence.
- `trust.toml` records exact trusted occurrences and their upstream contract, risk, evidence,
  issue, owner, independent reviewer, and expiry.
- `exclusions.toml` records exact H/T symbols excluded from proof, the unsupported feature, risk,
  compensating evidence, upstream tracking, owner, independent reviewer, and revisit deadline.
- `obligations.toml` records open, in-progress, discharged, or explicitly excluded proof
  obligations and their dependency graph.

`cargo xtask verify-trust` parses all five manifests with unknown-field denial. It validates their exact
schema envelopes, normal non-symlink source/evidence paths, Cargo and architecture ownership,
verification classes, stable IDs, typed actor principals, role-correct actor references, exact
provenance locators, issue syntax, independent identities, real calendar dates, review deadlines,
pinned upstream versions, exact locked evidence commands, status-dependent fields,
cross-references, and acyclic obligation dependencies.

`cargo xtask all` runs the locally executable trust scan and validates the actor, trust,
exclusion, and obligation manifests, but deliberately does not claim protected-base proof-impact
authorization. That two-step repository review remains available through the explicit
`cargo xtask verify-trust` command while hosted protected-runner enforcement is deferred.

Trusted constructs are reconciled one-to-one by owning crate, source file, line, symbol, and
scanner kind. An occurrence outside `peritus-tcb/src`, an occurrence without a record, a stale
record, or an ambiguous record fails the gate. Issue liveness is additionally checked by the
protected-branch/release authority because the repository-only checker deliberately performs no
network calls.

The trust and exclusion lists remain empty. The obligation manifest now carries the named,
in-progress proofs and executable evidence added by later slices, including A3 negotiation,
delivery, acknowledgement, chunk-conservation, terminal-ordering, canonical-wire, schema, and A2
conformance roots; `in-progress` is not a discharge claim. The actor registry contains the
implementation owner and independent final reviewer used by the proof-impact records.

## Canonical proof gate

The workspace proof commands and ordinary-Rust boundary audit are exact and policy-checked:

```text
cargo run --locked --package xtask -- ordinary-api-check
cargo verus verify --workspace --all-features --locked --check-toolchain --fwd-verus-args-to roots -- --rlimit 20
cargo verus verify --package peritus-agent --package peritus-app-protocol --package peritus-approval --package peritus-artifact-store --package peritus-budget --package peritus-codec --package peritus-collaboration --package peritus-context --package peritus-debugger --package peritus-eval --package peritus-evidence --package peritus-evolution --package peritus-gates --package peritus-git --package peritus-harness --package peritus-journal --package peritus-kernel --package peritus-leases --package peritus-memory --package peritus-migrations --package peritus-model-protocol --package peritus-network --package peritus-orchestrator --package peritus-patch --package peritus-policy --package peritus-process --package peritus-projection --package peritus-protocol --package peritus-provider-anthropic --package peritus-provider-compatible --package peritus-provider-core --package peritus-provider-google --package peritus-provider-openai --package peritus-quality-policy --package peritus-review --package peritus-role --package peritus-sandbox --package peritus-sandbox-linux --package peritus-sandbox-macos --package peritus-sandbox-windows --package peritus-scheduler --package peritus-secrets --package peritus-spec --package peritus-telemetry --package peritus-tool-protocol --package peritus-tool-router --package peritus-tools-fs --package peritus-tools-git --package peritus-tools-quality --package peritus-tools-shell --package peritus-trace --package peritus-types --package peritus-workspace --all-features --locked --check-toolchain --fwd-verus-args-to roots -- --no-cheating --rlimit 20
cargo verus build --workspace --all-features --release --locked --check-toolchain --fwd-verus-args-to roots -- --rlimit 20
cargo verus build --package peritus-agent --package peritus-app-protocol --package peritus-approval --package peritus-artifact-store --package peritus-budget --package peritus-codec --package peritus-collaboration --package peritus-context --package peritus-debugger --package peritus-eval --package peritus-evidence --package peritus-evolution --package peritus-gates --package peritus-git --package peritus-harness --package peritus-journal --package peritus-kernel --package peritus-leases --package peritus-memory --package peritus-migrations --package peritus-model-protocol --package peritus-network --package peritus-orchestrator --package peritus-patch --package peritus-policy --package peritus-process --package peritus-projection --package peritus-protocol --package peritus-provider-anthropic --package peritus-provider-compatible --package peritus-provider-core --package peritus-provider-google --package peritus-provider-openai --package peritus-quality-policy --package peritus-review --package peritus-role --package peritus-sandbox --package peritus-sandbox-linux --package peritus-sandbox-macos --package peritus-sandbox-windows --package peritus-scheduler --package peritus-secrets --package peritus-spec --package peritus-telemetry --package peritus-tool-protocol --package peritus-tool-router --package peritus-tools-fs --package peritus-tools-git --package peritus-tools-quality --package peritus-tools-shell --package peritus-trace --package peritus-types --package peritus-workspace --all-features --release --locked --check-toolchain --fwd-verus-args-to roots -- --no-cheating --rlimit 20
```

The workspace commands cover every opted-in V, H, and T package with the bundled solver-version
check and a pinned per-query solver limit. Each V/H package is then independently verified and
built with `--no-cheating`; the policy checker requires this package command inventory to equal
the architecture registry, so adding a formal package cannot silently omit the strict pass. T
packages run through the workspace pass without `--no-cheating` because their narrowly reviewed
trusted occurrences are instead reconciled one-to-one with `trust.toml`. This preserves a usable,
explicit TCB without allowing V/H proof cheats. Pinned `vstd` remains in the dependency trust
boundary. Focused modules/functions, package excludes, forwarded caller arguments, solver
bypasses, and `--no-verify` are rejected by workspace policy.

Pinned Verus labels `-V check-api-safety` experimental and currently applies it to imported
`vstd` as well as the selected Peritus crate. At the pinned revision this fails before assessing
Peritus because `vstd` models safe `Option::unwrap` with an `assume_specification` carrying its
real nontrivial precondition. Forwarding `--no-cheating` to all dependencies likewise rejects the
external specifications on which `vstd` is deliberately built. Neither failure describes a
Peritus source violation, so neither unusable whole-import mode is presented as release evidence.

`ordinary-api-check` instead applies a fail-closed, repository-owned policy to every V, H, and T
compilation source, including statically reached `#[path]` and `include!` inputs regardless of
file extension. It rejects `requires` on public safe executable functions and safe trait
implementations, contracts that an unverified implementation of a public safe trait could
violate, opaque `impl Trait` returns that could smuggle private preconditions, symbolic links,
and function headers it cannot delimit. Attribute-form `#[verus_spec(...)]` contracts receive the
same treatment. Conditional, qualified, aliased, custom-derive, and otherwise unmodeled source
expansions fail closed; only an exact built-in/Verus expansion inventory and trust-manifest-accounted
TCB attributes are permitted. Static code includes are recursively audited. Compile-environment
and embedded-data macros (`env!`, `include_str!`, and `include_bytes!`) are prohibited in formal
packages so mutable external bytes cannot evade the proof-impact inventory. Local macro
definitions are prohibited. Comments, literals, private verified helpers, spec/proof functions,
and explicitly `unsafe` call boundaries are distinguished token-by-token. Its adversarial tests
mirror the relevant cases in pinned Verus's own safe-API and attribute-syntax suites; where the
experimental pass attempts to prove a public safe-trait contract trivial, Peritus deliberately
uses the stricter rule of rejecting the clause. The experimental flag can replace this
conservative policy only after it can audit a repository crate without failing on imported
`vstd`. This behavior and the mirrored cases are anchored to the pinned upstream
[`safe_api.rs`](https://github.com/verus-lang/verus/blob/92f466f247f45128c630d1c843fd6e27d2115587/source/vir/src/safe_api.rs)
and [safe-API tests](https://github.com/verus-lang/verus/blob/92f466f247f45128c630d1c843fd6e27d2115587/source/rust_verify_test/tests/safe_api.rs), plus the
pinned [attribute-syntax tests](https://github.com/verus-lang/verus/blob/92f466f247f45128c630d1c843fd6e27d2115587/source/rust_verify_test/tests/syntax_attr.rs).

`just gate-a` combines these proof commands with ordinary all-target builds and tests, strict
Clippy and rustdoc, ordinary-API/source/architecture/trust/reproducibility checks, and the full
locked dependency advisory/license/source policy.
