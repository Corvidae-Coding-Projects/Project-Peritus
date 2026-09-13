# Proof-scope method ownership repair

This follow-up to local checkpoint `bb2f1ccdcc7a09da6c6a47d281ecadf76a6cc959` repairs GAP-01.
It changes the checker and source locators; it does not change product execution, specifications,
proof contracts, solver budgets, obligation statuses, or protected proof-impact approvals.

## Cause and repair

The checker previously appended an impl's self-type name to the module containing its source file.
For `accounting/usage.rs`, this produced `accounting::usage::AccountingState::apply_usage`, while
Verus reports `accounting::AccountingState::apply_usage` because the type is defined in
`accounting.rs`. `apply_work` had the same mismatch. The fresh compiler report was successful,
but exact scope comparison correctly rejected those incorrect registered symbols.

The source locator now distinguishes inline modules, trait declarations, and impl self types.
It follows a unique local nominal declaration or explicit import/re-export chain to the defining
module. It preserves each same-name declaration's token ordinal and verification mode. The
compiler gate still requires the exact registered symbol in its selection and a successful query
in the declared executable/proof mode; its implementation is unchanged.

The same correction applies to four ordinary source locators discovered by repository policy
validation: `MaterializationPlan::build`, `SqliteJournal::commit_approval_use`,
`SubscriptionState::deliver`, and `DaemonRuntime::start`. Existing ordinary trait-impl locators
remain supported. All six obligations retain their implementation source files and remain
`in-progress`; this repair does not discharge any obligation.

## Supported resolution and boundaries

Resolution follows explicit `crate`, `self`, `super`, grouped import and rename paths, including
explicit re-export chains, to a unique struct/enum/union definition in the repository's standard
module files or inline modules. Repeated paths, ambiguous bindings or files, type aliases,
glob-only owners, unresolved relative paths and unsupported self-type syntax fail closed.
There is no search by leaf name and no fallback to a similarly named compiler function.

Comments, literals, attributes, arbitrary macro token trees, function-local items, and malformed
nested associated-item paths cannot manufacture eligible method declarations or type bindings.
Only the recognized `verus! { ... }` wrapper is transparent. Skipping non-item token trees retains
same-name ordinals so it cannot substitute another declaration's mode.

This bounded source parser does not replace rustc name resolution, evaluate conditional import
activation, or establish custom module-path/macro expansion semantics. Standard file/module
correspondence remains a source-discovery boundary; conditional function eligibility and fresh
compiler selection/query checks remain required. An observed compiler scope is not independent
production-correspondence review, protected authorization, or proof discharge.

## Validation and delivery

An [independent source-bound review](formal-coverage-evidence/method-mapping/independent-review.md)
passed for all eight source/register paths. Final local Linux validation on those working sources:

| Check | Result |
|---|---|
| New source-owner regressions against the previous checker | Three initial tests failed, reproducing the incorrect mapping and unsafe owner acceptance |
| All xtask targets and features | 447 library tests, three CLI tests, and four release-staging tests passed; one existing library test ignored |
| CLI repository policy check (`xtask all`) | Passed as part of the CLI suite |
| Strict all-target/all-feature Clippy and workspace formatting | Passed |
| Actual `xtask ci-shard verus-verify-strict app-runner` | Passed: two registered symbols selected; 34 verified functions, zero errors |
| Compiler input recheck | All 4,627 captured files matched after execution |
| Regenerated formal inventory | Passed; 153 obligations across 66 formal packages, all obligations still in progress |
| Protected `verify-trust` | Failed on its existing reference to removed `verified_api/progress.rs`; GAP-02 remains open |

The test suite includes wrong owners, alias redirection, import order, re-export cycles, ambiguous
files/bindings, non-item declarations, nested associated-item decoys, preserved verification modes,
and exact compiler-selection/query rejection. Product source and the compiler report checker are
unchanged. Existing upstream Verus Clone warnings are retained in the raw log, not suppressed or
claimed as verified semantics.

The [commands and exit results](formal-coverage-evidence/method-mapping/commands.json),
[retained compiler result](formal-coverage-evidence/method-mapping/peritus-product-runner-verus-output.txt),
[complete scope](formal-coverage-evidence/method-mapping/selected-functions.json), and
[before/after source identities](formal-coverage-evidence/method-mapping/source-identities.json)
identify this repair. Checks ran before the local commit; the compiler snapshot correctly records
`matches_committed_sources: false`. This is local source-bound validation, not hosted final-commit
evidence or protected approval.

GAP-02 (protected proof-impact reconciliation) and GAP-03 through GAP-08 remain open. No GitHub
settings, reviewer requirement, deployment, push, or PR are part of this scoped repair. The full
formal-coverage goal remains incomplete.
