# Exact ghost documentation metadata

The ordinary API scanner previously rejected the conditional documentation-lint metadata needed
by the pinned Verus enum generator. The correction admits only
`cfg_attr(verus_keep_ghost, allow(missing_docs, reason = NONBLANK_STRING))`, with optional trailing
commas in either list. Both inner and outer syntax are scanned. Other predicates, nested or
additional conditional attributes, executable contracts, derives, broader lints and blank reasons
remain rejected. The independent function-header scanner continues to reject public executable
preconditions even when the accepted metadata precedes a Verus block.

## Independent review and verification

An independent Sol reviewer inspected both exact source identities, scanner callers and import
policy, then independently ran all 41 API tests and the actual ordinary-api checker. Parent ran
the same 41 tests, strict all-target/all-feature xtask Clippy, package formatting and ordinary API
scan. All passed. File counts differ between the two scans because other agents were adding
source modules; both inspected 14,649 ordinary executable entry points. An earlier invocation
used the unsupported command api-contract-check and failed before scanning; its log is retained
and is not passing qualification evidence. The supported ordinary-api-check supersedes it.

## Demonstrated generator boundary

The pinned generator synthesizes undocumented ghost projection methods. Placing the annotation
on WorkTerminal leaves six generated missing-docs errors; placing it on a macro invocation also
adds an unused-attribute error. A private work/terminal.rs leaf containing the fully documented
enum and a ghost-only inner annotation passes that scheduler checkpoint's 57 Verus checks.
Ordinary Rust does not enable verus_keep_ghost, so its documentation lint remains enabled.
The scheduler is still under proof development; the retained generator pass is not a final
scheduler qualification result.

The scanner checks exact syntax, not the generated origin or placement of an item. Keeping the
production annotation confined to that documented private leaf remains a source-review invariant.
No proof escape, executable contract exception or broader lint allowance is added by the scanner
change. Final trusted-checker fingerprints must include these two modified source files.
