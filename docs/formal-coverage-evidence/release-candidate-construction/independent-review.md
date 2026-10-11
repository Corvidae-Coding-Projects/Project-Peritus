# Independent review of release candidate construction

Verdict: PASS for the bounded constructor-contract increment. `/root` independently reviewed the eight-file patch implemented by `/root/sol_acceptance_completion`, the changed production files and their stored fields/getters, the complete new construction module, new test cases, the existing ConstructionError representation, and the actual derived-Clone verifier client. This is not a final obligation discharge or CI approval.

## Exact source identities

- Prior reviewed full package: 64 files, manifest `/tmp/peritus-parent-release377-source-exact.sha256`, SHA256 `14db526a3582c733e530ed94b4dab892cfeb125fb956203d17bf662f34c68a82`.
- Final frozen package: 65 files under `/tmp/peritus-sol-release-candidate-contracts-source-01`, manifest `/tmp/peritus-sol-release-candidate-final-full-package.sha256`, SHA256 `42cdfd534b84c5f7fa8f1e64aabb8957864343ebcb9336754da4b26806873918`.
- Eight changed files: `/tmp/peritus-sol-release-candidate-final-changed-source.sha256`, SHA256 `d2c55e0216afc5b5cc041033366f19ae2ce250e8b5ca9a76dedadc7e0162ab21`.
- Complete patch: `/tmp/peritus-sol-release-candidate-baseline-to-final.patch`, SHA256 `f2c3560c65629101a7639bd7665000fe7821e47d3fc83ff588d5cc5b20a1b6a4`.

All 64 prior identities match both the frozen preimage and shared package. All 65 final identities match the frozen final package, with no extra files. The actual changed set is exactly the eight-file manifest, including one new construction module. The supplied evidence manifest and every referenced artifact were hash-checked independently.

## Production correspondence

The ordinary runtime constructor bodies retain their prior checks, order, stored values, const qualification, and Result types. Four existing implementations move into a natural private construction module; they are still compiled for production. The increment strengthens postconditions and loop invariants without adding executable admission restrictions, replacement implementations, public executable requires, assumptions, trusted bodies, or checker changes. The existing narrow too-many-arguments allowance moves with its unchanged constructor; no new suppression is introduced.

Nominal IDs accept exactly nonzero 16-byte values, preserve every byte, and return the complete ZeroIdentity error kind otherwise. The private StableIdentity invariant and exact constructor/getter contracts carry through each nominal wrapper. The SHA-1 scan's loop invariant is exact existential nonzero-prefix correspondence; its increment and termination cover all 20 bytes. SHA-256 uses the existing verified 32-byte scan. Success specifies the exact Git hash format, all bytes in the selected variant, and absence of the other variant.

ReleaseVersion, PlatformIdentity and ToolchainIdentity accept exactly the inputs passing their real nonzero digest checks. Every scalar, enum and named digest is preserved. PlatformMatrix's admission is exactly Linux/macOS/Windows in those three slots; it preserves each complete PlatformIdentity and returns InvalidPlatformMatrix for a mislabeled slot. It does not impose a new architecture combination requirement.

ProfileIdentity, SchemaIdentity and ReleaseCandidate model every observable first error: zero revisions precede subsequent zero digest checks, while all successful fields are exact. The schema checks retain the original policy/evidence/report/artifact order. Several bad revisions are indistinguishable through the actual error representation, which stores only ConstructionErrorKind. The proof correctly characterizes that complete stored field rather than inventing parameter-specific diagnostics. The candidate container retains all nine supplied components; its own two admission checks remain source revision and manifest digest.

The constructor specs' closed field views read the actual private fields, and existing public getters expose those exact views. No equal-looking proxy object, arbitrary success predicate, or detached output is substituted. The actual derived Clone client calls `.clone()` on each of the twelve public Copy types with no precondition, proving whole-value equality and every field view. Its supplied strict run reports 13 verified functions including main. The existing derived implementations remain in production; no unsupported Clone limitation is claimed.

## Evidence and its limits

The implementer supplied strict pinned proof at 377 verified/0 errors, both feature-mode suites at 32 executable tests plus one compile-fail doc test each, strict all-target Clippy, formatting, and API scanning. These supplied results are distinct from parent integrated qualification recorded after this review. The unchanged verified-function count reflects stronger contracts, not an absence of new guarantees and not 377 newly discharged obligations.

The supplied negative control changes only require_revision's claimed error kind to a false ZeroDigest postcondition while leaving its real ZeroRevision behavior intact. Strict verification rejects the helper and dependent contracts at 373/4. Preimage and restored hashes agree; the final production package and derived-Clone client were verified afterward. This demonstrates rejection of a false contract, not a general proof-scope enforcement result.

The worker's whole-workspace layout failure came from the separate scheduler snapshot at 415 lines. Its changed release modules are within the existing limit. Parent integration must use the actual global layout checker on the stable shared tree; the worker's wc command alone is not treated as that gate.

Nonzero checks do not establish digest provenance, hash collision resistance, identity issuance, Git object existence, version-text correctness, platform/toolchain facts, or manifest construction. Exact supplied fields and observable constructor errors are proved; external assertions represented by those fields are not. The separate release evaluator's ghost predicate visibility remains feasible follow-up. ConstructionError::code also still lacks an exact string postcondition, although its unchanged source maps the proved kind to the documented literal; no exact diagnostic-string theorem is claimed here. Global proof records, trusted CI enforcement, final develop synchronization, PR publication and final-head runner qualification remain open.
