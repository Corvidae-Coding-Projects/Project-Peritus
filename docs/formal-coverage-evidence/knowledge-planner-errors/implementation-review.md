# Peritus run-knowledge exact planner error checkpoint

## Input and scope

- Starting package: `/tmp/peritus-parent-knowledge-admission-source`, whose 37-file manifest `/tmp/peritus-parent-knowledge-admission-full-package.sha256` has SHA256 `67b8b133b4fc0f0c5670333d6c05b7fa1736df1398ef763731d6ad7a78e57bb8` and verifies completely.
- Final frozen package: `/tmp/peritus-sol-knowledge-errors-source-01/crates/orchestration/peritus-run-knowledge`.
- Final 38-file source manifest: `/tmp/peritus-sol-knowledge-errors-source-final.sha256`, SHA256 `6854d6da0f6c0f6bd7b298f75b608a34e0937e32938ee266b233c6e00062e1d2`.
- Exact preimage-relative patch: `/tmp/peritus-sol-knowledge-errors.patch`, SHA256 `9da7f3a1d9abadd3bece6b27ec274d55e076f7284c55edc057f56fba2cddf038`.
- Changed files: `src/delta.rs`, `src/identity.rs`, `src/model.rs`, `src/model/delta.rs`, new `src/model/delta/errors.rs`, `src/plan.rs`, `src/plan/evaluation.rs`, and `tests/delta_packet.rs`.
- No manifest, lockfile, shared-worktree, persistence, digest, rendering, or external-effect changes.

## Proved correspondence

- `plan_invalidation` succeeds exactly when every supplied clarification target is valid. On failure, `clarification_targets_error` identifies an actual input index whose predecessors are valid and whose target is invalid, and binds the returned error to that exact target with all other optional fields absent. This makes the existential witness the unique first invalid target.
- `plan_delta_packet` retains its existing input-validity iff and successful `DeltaPacket::spec_refines` contract. Its error result now follows exact production precedence: snapshot role mismatch; current candidate mismatch; first stale section of the actual current snapshot; then the prior snapshot's first invalid clarification target.
- Role and candidate errors are exact plain errors. Current-snapshot and clarification errors carry exactly the selected section ID and no source or numeric fields.
- The first-stale proof is derived from the actual normalized same-revision `plan_invalidation` result. `restrict_exact_prefix`, `plan_matches_exact_entries`, `plan_all_reuse_matches`, and `normalized_plan_reuse_matches_freshness` connect each actual decision and its preceding actual prefix to the input-defined section freshness predicate. No output-dependent guard boolean or new executable precondition is used.
- `KnowledgeSectionId::matches_implies_equal` converts the already-proved exact 16-byte identity match to structural equality, allowing the executable plan entry's ID to satisfy the exact public error payload.

## Runtime tests and negative control

- The precedence regression combines multiple simultaneous invalid inputs. It checks role mismatch before candidate mismatch, candidate mismatch before later failures, the first stale current section (section 3) before invalid prior targets, and the first invalid target (section 9 after a valid section 3 and before another invalid section 10).
- Every returned error assertion checks `kind`, `section_id`, `source_id`, `expected`, and `actual`.
- Default and all-feature package suites each pass 29 tests.
- Replacing the stale-section error with a plain error causes strict verification to fail at the exact `current_snapshot_error` production assertion: 219 verified, 1 error. The final source was restored from the frozen copy, all 38 hashes matched, and the final strict proof was rerun successfully.

## Qualification

- Strict pinned Verus: 220 verified, 0 errors with `--no-cheating --rlimit 20`.
- Strict all-target/all-feature Clippy: pass with `-D warnings`.
- Package format: pass.
- Repository source layout: pass, 4310 files; largest touched production file is `src/model/delta/plan.rs` at 389 lines and the new module is 151 lines.
- Ordinary API scanner: pass, 3424 formal-boundary files and 14646 ordinary-safe executable entry points.
- The Verus run still reports six existing dependency Clone-spec warnings from `peritus-role` and `peritus-run-settlement`; this patch adds none.

## Honest boundary

The checkpoint proves the public planners' input-defined success/failure classification, ordered error selection, and complete error payloads. It relies on the independently reviewed knowledge admission/topology and successful-output contracts already present in the starting package. It does not prove source-digest authenticity, filesystem observation, persistence, context assembly, rendering, caller-wide end-to-end correspondence, or unrelated package behavior.
