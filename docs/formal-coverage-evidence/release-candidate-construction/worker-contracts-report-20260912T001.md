# Release candidate constructor correspondence checkpoint

## Verdict

PASS for the bounded candidate-family constructor and `require_revision` slice. The frozen final
package is derived from the exact reviewed release377 package, changes eight files, preserves the
ordinary public API and constructor branch order, and contains no trusted proof escape or new
executable precondition.

## Source identity

- Baseline root: `/tmp/peritus-parent-release377-source-exact`
- Baseline manifest: `/tmp/peritus-parent-release377-source-exact.sha256`
- Baseline manifest SHA-256: `14db526a3582c733e530ed94b4dab892cfeb125fb956203d17bf662f34c68a82`
- Baseline identity check: 64/64 files OK.
- Frozen final root: `/tmp/peritus-sol-release-candidate-contracts-source-01`
- Frozen final manifest: `/tmp/peritus-sol-release-candidate-final-full-package.sha256`
- Frozen final manifest SHA-256: `42cdfd534b84c5f7fa8f1e64aabb8957864343ebcb9336754da4b26806873918`
- Changed-source manifest: `/tmp/peritus-sol-release-candidate-final-changed-source.sha256`
- Changed-source manifest SHA-256: `d2c55e0216afc5b5cc041033366f19ae2ce250e8b5ca9a76dedadc7e0162ab21`
- Exact baseline-to-final patch: `/tmp/peritus-sol-release-candidate-baseline-to-final.patch`
- Patch SHA-256: `f2c3560c65629101a7639bd7665000fe7821e47d3fc83ff588d5cc5b20a1b6a4`

The changed files are `src/candidate.rs`, new `src/candidate/construction.rs`,
`src/candidate/git.rs`, `src/candidate/platform.rs`, `src/candidate/version.rs`,
`src/identity.rs`, `src/validation.rs`, and `tests/construction.rs`.

## Proved production semantics

- `require_revision(revision)` returns `Ok(())` iff `revision > 0`; every error is exactly
  `ZeroRevision`.
- Candidate, principal, review, and finding identity constructors accept iff any of the 16 input
  bytes is nonzero. Success retains all 16 bytes and failure is exactly `ZeroIdentity`.
- SHA-1 and SHA-256 Git commit constructors accept iff any byte of the respective input is nonzero.
  Success retains the exact bytes, selects the exact hash format, and leaves the other format
  absent; failure is exactly `ZeroIdentity`.
- Release version and platform identity construction accept iff their supplied digest is nonzero,
  retain every scalar/enum/digest field on success, and return exactly `ZeroDigest` on failure.
- The platform matrix accepts iff its three arguments name Linux, macOS, and Windows in that slot
  order. Success retains the exact three values; failure is exactly `InvalidPlatformMatrix`.
- Toolchain construction accepts iff all four supplied digests are nonzero and retains each digest
  in its named slot. Any rejection is exactly `ZeroDigest`; the public error type does not identify
  which zero digest was first.
- Profile construction accepts iff the revision is positive and digest nonzero. When both are bad,
  `ZeroRevision` precedes `ZeroDigest`; all success fields are exact.
- Schema construction accepts iff all four revisions are positive and the catalog digest is
  nonzero. Any zero revision precedes the later digest check; all five success fields are exact.
- Release-candidate construction accepts iff source revision is positive and manifest digest is
  nonzero. `ZeroRevision` precedes `ZeroDigest`; all nine supplied candidate components are retained
  exactly.
- Existing exact getter contracts remain the public observation path for every retained field.
- A separate verifier-client example called the actual compiler-derived `.clone()` on all twelve
  public candidate/identity structs and proved whole-value plus all field-view equality. It passed
  with 13 verified functions and no errors, so no manual Clone implementation or lint exception was
  added.

## Tested behavior

Four new scenarios cover all-zero nominal/SHA-256 rejection, late-byte nominal/SHA-1/SHA-256
acceptance and byte preservation, component getter round trips and zero-digest rejection, and
revision-before-digest error priority for profile, schema, and release candidate construction.

Both default and all-feature package suites pass 33 tests each, counting the compile-fail doc test.
Strict all-target Clippy and format checks pass. The ordinary API scanner passes across 3,499
formal-boundary files and 14,684 ordinary-safe executable entry points.

The repository-wide source-layout command stops only on the isolated workspace's unrelated
`crates/orchestration/peritus-scheduler/src/verified/reservations.rs` at 415 lines. Its source was
not changed. Every changed release-policy production module is below the 400-line limit; the
largest is `candidate.rs` at 274 lines, and the focused line-count audit exits 0.

## Proof evidence

- Strict final proof: `/tmp/peritus-sol-release-candidate-verus-final-01.log`, 377 verified, 0
  errors, SHA-256 `5555ce0918e2001e353f92cba06c6ee5f6f91174aeaed238be6b4f1abcb0c04a`.
- Derived Clone client: `/tmp/peritus-sol-release-candidate-clone-derived-probe-verus-02.log`,
  dependency 377/0 plus client 13/0. Probe source:
  `/tmp/peritus-sol-release-candidate-clone-derived-probe.rs`, SHA-256
  `42934a10276008d5568fe1743a52b6eb81310da4f402efb2138016fa4ad468a5`.
- Negative control: `/tmp/peritus-sol-release-candidate-negative-verus-01.log`, exit 101, 373
  verified and 4 errors. Changing only `require_revision`'s error postcondition from
  `ZeroRevision` to the false `ZeroDigest` claim made Verus reject the helper and dependent
  constructors. The positive file was restored to the exact pre-control SHA-256
  `c30708aebcb883435d0d721a70604eb9f22bf4666c80068b77c20e51d7e36c77`.
- Exact commands: `/tmp/peritus-sol-release-candidate-commands-20260912T001.txt`.

## Remaining boundaries

- Nonzero digest checks establish rejection of the reserved placeholder. They do not calculate or
  authenticate Git objects, version descriptors, platform profiles, toolchain artifacts, schema
  catalogs, or the release manifest.
- The release-candidate constructor retains a supplied nonzero manifest digest; it does not prove
  that digest was computed from the other eight fields. The separately reviewed fingerprint logic
  remains a deterministic byte reducer rather than an authenticated cryptographic hash.
- Nominal identity construction establishes exact nonzero bytes, not issuance uniqueness or
  external identity authenticity.
- The constructor contracts describe the complete observable `ConstructionErrorKind`. Where
  several inputs share one error kind, the public error has no field identifying the individual
  failing parameter.
- This checkpoint does not expand to global registers, fingerprints, CI, publication, or
  end-to-end provenance.
