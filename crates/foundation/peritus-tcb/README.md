# peritus-tcb

`peritus-tcb` is the sole source location in which Project Peritus may eventually place narrowly
reviewed Verus escape hatches and external specifications. Its verification class is `T` because
the crate defines that policy boundary, not because every item in it is trusted.

The A1 baseline contains no trusted Verus construct, external specification, operating-system
adapter, or authority decision. The crate depends only on pinned `vstd`, whose prelude and macro are
required for Cargo-Verus to verify a crate; it does not depend on another Peritus crate. Its package
metadata sets `verify = true`.

## Public contract

The crate exposes the repository-relative locations of the five verification manifests. This is
an enumeration convenience only. The manifests, not Rust constants, are the authoritative
inventories, and consumers must parse them according to [`verification/README.md`](../../../verification/README.md).

## Boundary rules

- Trusted constructs may be introduced only in this crate and only with a matching
  `verification/trust.toml` entry.
- Trusted operations and proof-forging constructors must use scanner-recognized canonical
  spellings. Imports, reexports, aliases, and inline-module/impl nesting are prohibited so each call
  maps to one unambiguous manifest record.
- A trust entry must identify one exact source occurrence and its review and evidence. A broad
  module, file, crate, or dependency allowlist is invalid.
- The crate must not decide acceptance, authorization, policy, or any other domain outcome.
- Foundation types and `vstd` are the only permitted future normal dependencies. A dependency must
  be justified by source that actually needs it.
- Adding an escape hatch, dependency, or exported behavior requires trust-manifest reconciliation,
  full Verus verification, ordinary Rust tests, and independent review.

## Verification

Run the scoped ordinary-Rust checks with:

```text
cargo test --locked -p peritus-tcb
cargo clippy --locked -p peritus-tcb --all-targets --all-features -- -D warnings
```

Run the non-focused proof check with:

```text
cargo verus verify --package peritus-tcb --all-features --locked --check-toolchain --fwd-verus-args-to roots -- --rlimit 20
cargo verus build --package peritus-tcb --all-features --release --locked --check-toolchain --fwd-verus-args-to roots -- --rlimit 20
```

Run `cargo run --locked --package xtask -- ordinary-api-check` to audit every formal crate's
ordinary-safe executable boundary. The workspace rationale for separating that repository-owned
audit from pinned Verus's experimental whole-import API pass is documented in
[`docs/formal-foundation.md`](../../../docs/formal-foundation.md).

The workspace-wide trust checker is responsible for reconciling source occurrences against the
versioned manifests. An empty list means exactly zero registered records; it is never proof that an
untracked obligation was discharged.

## Focused checks

From the repository root:

```sh
CARGO_BUILD_JOBS=2 cargo test --locked --package peritus-tcb
```
