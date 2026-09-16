# Independent review: proof-scope method ownership repair

- Date: 2026-09-13
- Reviewer task: `/root/method_mapping_review`
- Worktree: `/home/doll/Project-Peritus/.worktrees/formal-coverage`
- Base commit: `bb2f1ccdcc7a09da6c6a47d281ecadf76a6cc959`
- Review mode: source inspection and retained-result inspection; no repository edits or builds were performed by this reviewer

## Verdict

**PASS for the bounded GAP-01 method-mapping repair at the exact source hashes below.**

The change fixes the reported ownership error without weakening symbol comparison. A method in an
out-of-line impl is now mapped through its syntactic self type and a unique, explicit same-crate
binding chain to the nominal type's defining module. Wrong impl-file owners, aliases as owners,
other same-named types, and wrong compiler selection/query names remain rejected. I found no open
correctness or scope blocker in the reviewed source.

This verdict covers source-locator ownership and its regression tests. It does not discharge an
obligation, approve proof-impact reconciliation, establish production correspondence, or qualify a
deployment, hosted check, push, merge, or release.

## Reviewed source identities

| Path | SHA-256 |
|---|---|
| `xtask/src/trust_lexer.rs` | `a54fee1a5d25f4edde17b04eb8d90e314b1a27d9d3bd82a814b0831eac3690db` |
| `xtask/src/trust_lexer/declarations.rs` | `7b09703862c06d560fc242f320d56f698591615394af66bd951c403a96410692` |
| `xtask/src/trust/manifest_symbol.rs` | `00617c0b8a19945b81a1c77f8ecf5314f0d0bb9d79e0d995e1f76da93bfe3948` |
| `xtask/src/trust/manifest_symbol/owner.rs` | `888c171ede0ddae297b93c2075269e7a8c117bf252e325f0961743d15ee9c7ca` |
| `xtask/src/trust/manifest_tests.rs` | `ad99fcb12d335ad5fad7859ef9d86cfdea475ad5770eeef138b8458e454afa93` |
| `xtask/src/trust/manifest_tests/method_owners.rs` | `d8a1508f8e383ea1426ca6e7ce569400e6fc0d5e21ef361cac9772c582c710a2` |
| `xtask/src/ci_shard/proof_scope/report/tests.rs` | `0f0bf9361098a2cdab8e2dcc1ca93e7607fb96a64ea3ffdb7d9e64f7a2221a5a` |
| `verification/obligations.toml` | `6796854430d100cee4a8440f786d995f55540461cf285dbcae806d5fe0a02aeb` |

The exact compiler-report checker itself is unchanged:
`xtask/src/ci_shard/proof_scope/report.rs` has SHA-256
`db35730b6abd353a047bdc81bf90528fb92a9e7c6267d0a60bb3fac02ae90f7f`.

## Substantive review

The lexer now distinguishes inline module owners, trait declaration owners, and impl self-type
owners. It retains the same-name declaration ordinal before discarding an unsupported candidate,
so the independently parsed Verus mode and configuration cannot slide from one same-named
declaration to another. Owner grammar permits inline modules followed by at most one terminal trait
or impl owner. Functions, arbitrary blocks, unsupported impls, and nested associated-item decoys do
not yield eligible candidates.

Impl resolution supports a simple nominal self-type path, including generic impl headers and
generic arguments on the self type. For an ordinary trait impl it takes the concrete self type after
the top-level `for`; this preserves the existing
`VerifiedReleasePolicyAdapter::evaluate` locator. It resolves a local nominal directly or follows
explicit imports, grouped imports, renames, `crate`/`self`/`super` paths, and exact re-export chains.
Every chain must remain in the owning crate and end at exactly one struct, enum, or union
declaration. A visited set rejects re-export cycles.

There is no leaf-name search, suffix match, owner alias acceptance, or fallback to the impl file.
Missing or duplicate bindings, duplicate nominal declarations, ambiguous `foo.rs`/`foo/mod.rs`
files, glob-only imports, type aliases, cycles, and unsupported self-type grammar yield no owner.
Comments and literals are excluded by tokenization. Attribute token trees and every macro token
tree except the recognized `verus! { ... }` wrapper are skipped with balanced delimiter handling
for braces, parentheses, and brackets; skipped same-name `fn` tokens still advance the ordinal.

The obligation edits are locator-only. They correct four ordinary methods to their nominal
definitions (`MaterializationPlan`, `SqliteJournal`, `SubscriptionState`, and `DaemonRuntime`) and
correct both the obligation and Verus-evidence symbols for `AccountingState::apply_usage` and
`AccountingState::apply_work`. The recorded implementation files, commands, specifications,
statuses, and solver budgets are unchanged.

The compiler-report regression independently varies selected and queried names. Acceptance occurs
only when both are the exact canonical defining-owner symbol. Because `report.rs` is unchanged, the
repair does not introduce alias, suffix, or fuzzy compiler-report matching.

## Deliberate supported-subset boundary

This resolver is a bounded lexical resolver, not rustc name resolution. It assumes the checker's
existing conventional source layout under `src` or `tests`: `lib.rs`/`main.rs`, `foo.rs`,
`foo/mod.rs`, and inline modules. It does not reconstruct `mod` declarations, Cargo target module
graphs, custom `#[path]` modules, external/prelude resolution, macro expansion, or complex
qualified/associated self types. An unqualified explicit import is interpreted relative to the
current lexical module and must lead through the conventional same-crate source layout to a unique
nominal declaration. These assumptions can conservatively reject valid Rust and mean that source
layout is still part of the checker's trust boundary.

The binding collector does not evaluate `cfg` activation or procedural attribute transformation.
It ignores tokens inside attributes and macros, then inspects the following syntactic item. For
Verus evidence, the existing declaration-configuration requirement and the fresh compiler gate
provide the final check: the exact registered symbol must be selected and must have a successful
solver query in its declared mode. Ordinary obligation locators are source locators and do not gain
a compiler-proof claim from this repair.

## Validation evidence inspected

I inspected the retained command records and raw outputs under
`docs/formal-coverage-evidence/method-mapping/` after the final source changes:

- `cargo clippy --locked --package xtask --all-targets --all-features -- -D warnings`: exit 0.
- `cargo test --locked --package xtask --all-targets --all-features`: exit 0; 447 library tests,
  three CLI tests, and four release-staging tests passed; one library test was ignored.
- `cargo fmt --all -- --check`: exit 0.
- `target/debug/xtask ci-shard verus-verify-strict app-runner`: exit 0; two registered symbols were
  selected and 34 solver-query functions were accepted, with zero verification errors.
- `target/debug/xtask formal-inventory`: exit 0; 153 obligations across 66 formal packages, all
  still in progress.
- The post-command source recheck reports all 4,627 captured inputs unchanged.

The protected `target/debug/xtask verify-trust` command still exits 1 on the already recorded
removed `verified_api/progress.rs` reference. That is GAP-02 and prevents a full trust-gate or
formal-coverage completion claim; it does not contradict the exact app-runner method-scope result.

I also ran a read-only `git diff --check` against the reviewed worktree and observed no whitespace
errors.
