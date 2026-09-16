# Independent obligations-foundation review

## Scope and provenance

- Read-only review of the frozen `peritus-obligations` package at `/tmp/peritus-parent-obligations-foundations-source/crates/orchestration/peritus-obligations`.
- Nine-file change manifest: `/tmp/peritus-parent-obligations-foundations-source.sha256`, manifest SHA-256 `1b82b629c7848a984c98606e0487f885f1d3bff4806dbfb9219ac968c86e66b5`.
- Full 36-file package manifest: `/tmp/peritus-parent-obligations-foundations-full-package.sha256`, manifest SHA-256 `1f079cae7458024bcfcd08468cdd3f663c1bb1dd5f1c9adaa2883da6b07690fc`.
- Both manifests were checked against the frozen source and passed. An initial nine-file check was invoked from the package subdirectory even though its paths are workspace-relative; that invocation reported missing paths only. Re-running from `/tmp/peritus-parent-obligations-foundations-source` passed all nine entries.
- Compared the full frozen package against `/tmp/peritus-parent-obligations-foundations-preimage` and inspected the supplied patch. The change set is exactly the nine manifest paths: six modified source files, two new private proof/model leaves, and one new integration-test file.
- Inspected the supplied verification/test/lint/format and mutation logs. I did not independently execute Cargo or Verus for this review.

## Bounded verdict

PASS for the stated foundation increment. The executable branches remain equivalent to the preimage except for replacing derived `PathId` ordering with explicit lexicographic comparison over the same complete 32 digest bytes; `SchemaDirection != variant` became an equivalent two-variant `!matches!` expression. I found no dropped fields, new runtime guard, executable precondition, alternate implementation, proof escape, panic, unsafe block, or newly introduced lint allowance in the nine files.

The contracts establish the following source-bound facts:

- `ConditionObservation`, `ObligationError`, and `ObligationLimits` constructors/getters retain every stored field. Limits admission is an exact iff, and every rejection is the exact plain `InvalidLimit` shape.
- `PublicTaskSource::from_parts` has exact size admission and retains all bytes, the supplied digest, and revision. `ClauseProvenance` and `PublicClause` retain all digest/revision/ordinal/span and clause-byte fields.
- `PathMention::new` admits exactly nonempty paths within the supplied byte bound and retains identity, all exact path bytes, and role. `PathRole::requires_candidate_evidence` is tied exactly to the two output roles.
- Path collection validation checks the maximum first, then the first non-increasing adjacent pair. It compares all 32 `PathId` digest bytes, distinguishes equality from descending order, and proves successful strict ordering and global uniqueness. The preimage's derived `Ord` traverses the same one-field `PathId(Sha256Digest { bytes: [u8; 32] })` representation lexicographically, so the runtime result and error priority are preserved.
- `ObligationSpec::validate` accepts exactly the intrinsic schema-direction shape; the only direction enum variants are Request and Response, making the rewritten matches expressions equivalent to the prior comparisons.
- `RequirementEntry::new` preserves the exact ID, clause, typed specification, and ordered path sequence; its admission iff combines specification shape, maximum path count, and complete-ID order. Invalid shape precedes path errors, while path validation retains size-first and earliest-pair error precedence. Its type invariant also records uniqueness, which follows from strict order over fixed-width 32-byte identities.
- The five manual Clone implementations preserve complete semantic content: public-source bytes/digest/revision; clause bytes/provenance; path ID/exact bytes/role; all `ObligationSpec` variants and nested schema content; and entry ID/clause/specification/ordered path content. The supplied negative mutation that substitutes only `PathMention.role` fails the complete-content postcondition at 224 verified/1 error; restored source passes 225/0.

Actual caller inspection confirms `RequirementLedger::extract` constructs provenance and exact clause bytes before calling `RequirementEntry::new`; the entry constructor calls the verified specification and path validators; ledger cloning consumes these manual Clone implementations; and public getters expose the retained fields.

## Evidence inspected

- Strict Verus log `/tmp/peritus-parent-obligations-foundations-verus5.log`: 225 verified, 0 errors. It contains two warnings in dependency `peritus-run-settlement`, not this package's Clone implementations.
- Restored mutation-control log `/tmp/peritus-parent-obligations-foundations-verus6-restored.log`: 225 verified, 0 errors.
- Negative role mutation `/tmp/peritus-parent-obligations-foundations-negative-role.log` plus JSON metadata: 224 verified, 1 error at the `PathMention::clone` complete-content postcondition; metadata records original source SHA `ebd021b2103e4fd78787bd34f6d31bfcd5d0590d3e041f2a7747095c4af2c42c`, mutant SHA `b8d1ba1e7ee5a724a44bbec73922be17c5271b9e24d1311db1e9f3ca69790ca5`, negative exit 101, restored exit 0.
- Default and all-features test logs each report 25 passed, 0 failed, including binary path bytes, all five roles, exact clone content, limit/source boundaries, and path size/first-pair precedence.
- Strict Clippy log completes successfully; rustfmt check log is empty and successful as supplied.

## Boundaries and open work

- The verified `from_parts` contract treats its digest as supplied data. The ordinary public constructor's SHA-256 computation, digest authenticity, and source provenance are not verified here.
- `RequirementLedger::extract`, ledger canonical hashing, alternative topology, condition/evidence lookup, qualification traversal, and end-to-end qualification remain ordinary outside this increment.
- The entry theorem validates the actual stored path sequence by stable identity digest; it does not establish that a `PathId` is the hash of `PathMention::exact` bytes.
- The proof establishes exact behavior of these constructors/getters/validators and Clone implementations only. It is not a whole-package or end-to-end formal-verification claim.
