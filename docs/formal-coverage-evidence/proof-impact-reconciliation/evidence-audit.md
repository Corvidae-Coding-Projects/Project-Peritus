# GAP-02 proof-impact, review, and obligation evidence audit

Date: 2026-09-13

Evidence snapshot: committed tree `d0e4f0cf0943c7194adaaeabcb11fc70badc3fe8`, inspected before later concurrent parent edits to `xtask/` appeared in the shared worktree.

Repository: `/home/doll/Project-Peritus/.worktrees/formal-coverage`

Mode: read-only repository audit. No repository file, Git ref, build output, network setting, or external service was changed. This report is the only file written, under `/tmp` at the parent agent's request.

Agent task: `/root/impact_evidence_audit`

Assigned runtime profile supplied by the parent: `gpt-5.6-sol`, reasoning effort `xhigh`.

Identity limit: no canonical collaboration session number or repository-qualified principal for this task was supplied to the audit. The task/model/effort above therefore must not be copied into `actors.toml` as if it were authenticated actor provenance, and this report is not a PCR approval.

## Verdict

GAP-02 is a real append-only authorization gap, not a one-line stale-path repair. The current `proof-impact.toml` is internally authentic for the historical PCR-0005 candidate `a6994ce0`, but its current-source view does not describe PR-head commit `d0e4f0cf`. The current obligations register is truthful: all 153 entries remain `in-progress`; no evidence inspected supports changing any entry to `discharged` or `excluded`.

The existing bounded reviews are useful retained evidence, but none is a protected PCR authorization for the complete current tree. PCR-0005 and ACTOR-0001 through ACTOR-0004 must remain immutable. A fresh, separately staged PCR with the actual owner identity and a new independent reviewer is required. Another agent's review task must not be re-attributed to this audit, the parent, or a newly invented actor.

## Git and source identities

| Role | Commit | Tree | Meaning |
|---|---|---|---|
| Draft PR #74 base | `8e8cbb1bcf0de9787d86c7be3d6fc1f2079f3493` | `3e5febb6c672bf24e86a1c8823f2e931fb33aef1` | Supplied/current `origin/develop` base for this audit |
| PCR-0005 authorization base | `e6f8adb68972c6fcf96189674f034b260bbdb2f8` | `7bdfbba811ad1222eddce7fe34ff62c420d75fdb` | Historical base named by the detached verdict |
| PCR-0005 implementation | `a6994ce09a67b6631318d88659cd4df5ed5e81f7` | `0d3c5a78fe8d8e97620378f4f365b63e90ed673bf` | Exact historical candidate approved by ACTOR-0004 |
| Stabilization checkpoint | `bb2f1ccdcc7a09da6c6a47d281ecadf76a6cc959` | `79187647ff871b43f2f289ed21a459ae39d4f68d` | Ledger and scheduler checkpoint |
| Current PR head | `d0e4f0cf0943c7194adaaeabcb11fc70badc3fe8` | `73791a6d6079fd026cf6595fdc37dcf565456212` | GAP-01 method-owner repair on top of the checkpoint |

At the audit snapshot, `HEAD` was exactly `d0e4f0cf` and the worktree had no tracked modification. Its visible untracked files were the existing machine/integration families `.claude/`, `.codex/`, `.crosslink/`, `.mcp.json`, and `AGENTS.md`. A post-report status check observed concurrent parent work under `xtask/`; those later bytes are outside this snapshot and require a new final freeze.

The 3,390 current source rows in `verification/proof-impact.toml` match the raw Git blobs at PCR-0005 implementation commit `a6994ce0` exactly: 3,390 matches, zero mismatches, zero missing. This independently confirms that PCR-0005's source transitions describe its declared implementation tree.

The same historical source rows are already stale at draft-PR base `8e8cbb1b`: 3,222 match and 168 have different raw-byte hashes. None is missing there. The proof-impact file at `8e8cbb1b` has SHA-256 `39135a20188b9415102c51c04e44f53fad27ea73a0775bbb4623a251b88a6d44`, identical to the current file, so the base carried inherited record drift rather than a new reviewed transition.

At current head `d0e4f0cf`, 3,028 of the 3,390 rows match, 361 have different raw-byte hashes, and one path is absent:

`crates/app/peritus-product-runner/src/verified_api/progress.rs`

The missing path occurs in two different historical/current roles:

- `proof-impact.toml:1120` is the current `sources[]` row. It is stale and must disappear from the next current-source projection.
- `proof-impact.toml:4629` is PCR-0005's historical `current` snapshot. It is authentic for `a6994ce0` and must remain unchanged. A new PCR must record its exact previous snapshot and a removal with no `current` snapshot.

A conservative Git-diff classification from `a6994ce0` to `d0e4f0cf`, limited to current V/H/T package roots, package manifests, and the eleven shared inputs, finds 361 modified registered inputs, 251 added formal `.rs`/package-manifest inputs, and the one deletion above. All 251 additions are under current formal package roots; the checker's Cargo-reachable discovery remains the final authority before these counts are encoded.

The architecture adds one formal package since PCR-0005: `peritus-local-socket`, class `H`, at `crates/runtime/peritus-local-socket`. Its eight conservative inputs are its `Cargo.toml` and seven Rust sources. As a result, every shared-input affected-package set is stale even when its bytes did not change. All eleven shared rows need a 66-package affected set rather than the historical 65-package set:

- `.cargo/config.toml`
- `Cargo.lock`
- `Cargo.toml`
- `architecture.toml`
- `rust-toolchain.toml`
- `toolchains.toml`
- `verification/actor-provenance.json`
- `verification/actors.toml`
- `verification/exclusions.toml`
- `verification/obligations.toml`
- `verification/trust.toml`

Four of those eleven also changed bytes (`Cargo.lock`, `Cargo.toml`, `architecture.toml`, and `verification/obligations.toml`). The other seven require scope-only transitions. If the normal checker confirms all 251 added paths are Cargo-reachable, the finite PCR transition is 620 rows: 361 modifications, 251 additions, one removal, and seven additional scope-only changes. Its applied current inventory would contain 3,640 rows, of which 619 would name the new PCR as their current `change_id`. These are audit-derived proposal counts, not a substitute for the parent's canonical checker inventory.

## Historical PCR-0005 review is authentic but not current authorization

`verification/reviews/PCR-0005.toml` is a coherent retained detached verdict:

- raw verdict hash `0b63d109a1a41428d369d5a4e003ec2ade0ef93c6ec263eb4d72832525c0d1b0` exactly matches the PCR-0005 reference;
- reviewer `ACTOR-0004` and principal `Corvidae-Coding-Projects/Project-Peritus/session/23/task/root/workbench_formal_review` match `actors.toml` and `actor-provenance.json`;
- implementation commit/tree are exactly `a6994ce0` / `0d3c5a78`;
- all 130 gate rows say `passed` and all 130 referenced outputs exist at their exact hashes;
- all four blocking high findings are retained and marked `fixed`;
- the review report and all finding artifacts are present at their exact hashes;
- the 139-row artifact inventory equals the 139 regular files in `verification/reviews/PCR-0005/`, with no missing, unclaimed, or hash-mismatched artifact.

This evidence supports the historical `approved` state of PCR-0005. It does not support calling `8e8cbb1b`, `bb2f1ccd`, or `d0e4f0cf` approved by ACTOR-0004. Reusing PCR-0005, changing its 3,374 source transitions, changing its verdict, or reusing ACTOR-0004 as the next reviewer would violate the documented immutable-prefix and fresh-reviewer contracts.

Current actor records are also internally coherent. `actor-provenance.json` has raw SHA-256 `fe8559d6c7018bb444007a772f154946086f066c4983a92e60b87374f21157a6`, and all four actor entries refer to that exact hash. Their actual retained scopes are:

- ACTOR-0001: Crosslink owner `6ME5`, issue 3/session 2/task `/root`;
- ACTOR-0002: A1 reviewer, session 2 task `/root/a1_final_gate_review`, `gpt-5.6-sol`/`xhigh`;
- ACTOR-0003: Crosslink owner `krvx`, issue 66/session 23/task `/root`;
- ACTOR-0004: PCR-0005 reviewer, session 23 task `/root/workbench_formal_review`, `gpt-5.6-sol`/`xhigh`.

None of those records names issue 75 or `/root/impact_evidence_audit`. The current Crosslink agent snapshot names `4jyS`, which is also the recorded creator of issue 75 in the retained local issue-row evidence, but a short agent ID alone is not enough to create a Crosslink actor record. A new owner entry is needed unless the final implementation owner can prove it is one of the existing canonical principals. The next reviewer always needs a fresh canonical collaboration principal and provenance entry.

## Current retained review claims

There are 38 top-level `docs/formal-coverage-evidence/*/review.json` files:

- 36 explicitly include `not-obligation-discharge` in their status;
- the stabilization checkpoint says `two-patches-reviewed-and-integrated; full-goal-incomplete`;
- the remaining `ci-snapshot/review.json` says `READY`, but only for a bounded 13-file snapshot at head `1aa9282f` / tree `3e5febb6`; its companion retention record explicitly says `no obligation discharge or final trusted CI approval`.

Thus, no retained review JSON truthfully authorizes a discharged obligation or the complete current proof-impact transition. Any global reading of `ci-snapshot`'s `READY`, or of prose such as a bounded increment being "closed," would be unsupported outside the record's stated scope.

The strongest current-source reviews remain useful as inputs to a fresh reviewer:

1. `method-mapping/independent-review.md` is a real bounded source review by task `/root/method_mapping_review`. Its eight source/register hashes all match current head, and all 15 entries in `artifact-sha256.json` match their files. It explicitly refuses obligation discharge and proof-impact approval. Its `source-inputs.json` records `git_commit = bb2f1ccd` and `matches_committed_sources = false`; therefore it is source-hash-bound but must not be described as a detached commit/tree authorization for `d0e4f0cf`.
2. `stabilization-checkpoint/review.json` and the three retained review texts are real bounded checkpoint records. All eight `changed_paths.after` hashes still match current files. They explicitly retain the failed trust gate and the full-goal-incomplete status. They are not PCR verdict artifacts and have no fresh protected-base authorization.
3. The older domain review JSON files retain exact package/source manifests and often a collaboration task label such as `/root`, `/root/sol_acceptance_completion`, `/root/sol_runtime_phase`, or `/root/sol_readiness_correspondence`. Those labels are not canonical `<repository>/session/.../task...` actor principals. They may be cited as bounded review evidence, but must not be enrolled retroactively or re-attributed as the next protected reviewer.

No fabricated PCR verdict was found. The unsupported action would be converting any bounded review, task label, test pass, or source manifest into current PCR approval without a fresh source-bound reviewer and detached verdict.

## Obligation status result

`verification/obligations.toml` parses to exactly 153 unique entries. Current status and ownership counts are:

- `in-progress`: 153
- `open`: 0
- `discharged`: 0
- `excluded`: 0
- owner `ACTOR-0001`: 153

All 153 primary `source_file` paths exist. All nested evidence source paths exist. There are 119 entries with at least one `verus-proof` evidence locator and 34 without one. A locator records eligible evidence; it does not say a command passed or a theorem discharges the full obligation. No entry has evidence sufficient to override its explicit bounded review limits and current incomplete campaign state.

The truthful GAP-02 record action is therefore to leave every status `in-progress` and keep the conditional `reviewer`, `review_date`, and `exclusion_id` fields absent. The 16 OBL-0209 through OBL-0224 records should retain their current in-progress status. The GAP-01 edits changed only six locators and did not change a statement, source file, status, or discharge claim.

## Exact reconciliation records still required

1. Preserve PCR-0001 through PCR-0005, `verification/reviews/PCR-0005.toml`, and its review directory byte-for-byte.
2. Use the parent's canonical Cargo-reachable inventory on one frozen implementation commit/tree. Do not author final records from an unfrozen working directory.
3. Create the actual owner provenance if the implementation principal is new, plus one fresh independent reviewer provenance. Append actor entries; do not alter an old principal or attribute an old task's review to the new actor. Refresh the shared provenance-file digest in every actor entry because `actors.toml` content-addresses the whole provenance record.
4. Append PCR-0006 in a separate protected preauthorization stage. Its source transitions must continue the exact PCR-0005 snapshots, including a real removal transition for `verified_api/progress.rs`; additions have no `previous`; byte changes and scope-only shared-input changes have exact `previous` and `current` snapshots.
5. Update only the applied `sources[]` projection to the new current identities and PCR-0006 `change_id` values. Do not delete or rewrite PCR-0005's historical transition for the removed progress file.
6. Because the changed shared inputs affect all 66 V/H/T packages, PCR-0006 needs the exact class-correct ordinary-test and Verus command for each package: 132 evidence rows and, in the detached verdict, 132 corresponding gate rows/output artifacts. All must bind the frozen reviewed candidate and actual results.
7. Add `verification/reviews/PCR-0006.toml`, a non-empty review report, all exact gate outputs, finding detail/evidence artifacts, and a complete hash-bound directory inventory. Bind the real authorization base, implementation commit/tree, transition digest, gate digest, finding digest, and artifact digest. Do not record approval before the fresh reviewer issues it.
8. Keep all 153 obligations `in-progress`. Refresh `docs/formal-coverage-inventory.json` and checkpoint prose as audit/current-source evidence only. Mark GAP-02 complete only after the protected-base verify-trust path accepts the authorized implementation state.

The repository contract requires the PCR review to be authorized before its implementation transition is applied. A same-head edit of current sources plus a newly written approving record is not valid. The parent is handling the separate protected preauthorization proposal and exact base mechanics.

## Reusable artifacts and limits

No proof-impact reconciliation generator, PCR-0006 draft, or approval script was found in the target evidence tree or in the narrowly searched `/tmp/peritus-*` scripts. The reusable pieces are:

- the checked-in validator and exact schema in `verification/README.md` and `xtask/src/trust/manifest_impact*`;
- PCR-0005 as a structural example only, without changing or copying its identity/reviewer;
- `/tmp/peritus-sol-formal-deployment-audit-20260913T001.md`, a sound high-level deployment audit whose fingerprint counts are stale (it reports 357 mismatches and a 215-path lower bound before the final GAP-01 checkpoint; current observations are 361 and 251);
- `/tmp/peritus-sol-obligation-record-validate-001.py` through `-003.py`, historical validators for the OBL-0209 through OBL-0224 draft. They correctly enforce `in-progress`, ACTOR-0001, issue #75, source/evidence locators, and DAG shape, but they neither generate PCR transitions nor authorize review;
- the method-mapping and stabilization artifact bundles described above, which are exact bounded review inputs rather than protected approval.

## Reviewed path hashes at current head

| Path | SHA-256 |
|---|---|
| `verification/proof-impact.toml` | `39135a20188b9415102c51c04e44f53fad27ea73a0775bbb4623a251b88a6d44` |
| `verification/actors.toml` | `2fc6645560d1d95118d5cd9f08260953316ede9971e146ef06103f8b9462dad2` |
| `verification/actor-provenance.json` | `fe8559d6c7018bb444007a772f154946086f066c4983a92e60b87374f21157a6` |
| `verification/obligations.toml` | `6796854430d100cee4a8440f786d995f55540461cf285dbcae806d5fe0a02aeb` |
| `verification/reviews/PCR-0005.toml` | `0b63d109a1a41428d369d5a4e003ec2ade0ef93c6ec263eb4d72832525c0d1b0` |
| `verification/reviews/PCR-0005/review-report.md` | `601f63aaa74b44bc1d20a25a1d042027d2b70ce9060615d23a0cca21c9ed45f9` |
| `docs/formal-coverage-audit.md` | `f8f2455a8caa91dcf41b969d75949f53df61f265c92573df3eb792d44799831d` |
| `docs/formal-coverage-checkpoint.md` | `34930939c1ccf193159e74b2d60b105c9f4ad6e4e2161925d3f171e59fde9ccb` |
| `docs/formal-method-mapping-fix.md` | `46aec1588e4bb5fe148b418f086791020573660ae9310417575fb7b92bd3eaf8` |
| `docs/formal-coverage-evidence/method-mapping/independent-review.md` | `8e621cb21757cc8399ed1656daa60416b9944c822e965b026922aaf9ed24ae46` |
| `docs/formal-coverage-evidence/method-mapping/artifact-sha256.json` | `260590296d7d77e27d60c8f00d0a6dfb1b0308d599324f42870bf9522a6b1c76` |
| `docs/formal-coverage-evidence/method-mapping/source-identities.json` | `0772b96a609a15bec98ce50e2bb1768345e61906c63e59466c78bca6e9bb297b` |
| `docs/formal-coverage-evidence/stabilization-checkpoint/review.json` | `d78d5a8d9ad9401d0b687f798a688575256662cba4c29350a0f134a27591065e` |
| `docs/formal-coverage-evidence/stabilization-checkpoint/peritus-checkpoint-scope-review.md.txt` | `70c56f10d1605a5fe6b69f9714364b541130b90180cf12c98e51cd9a774bf5e1` |

Best next record action: freeze and inventory the actual final candidate, then prepare the separate non-approving PCR-0006 preauthorization proposal with explicit pending reviewer state. The approval must come from the newly enrolled independent reviewer after reviewing that exact candidate.
