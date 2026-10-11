# GAP-02 proof-impact authorization audit

## Audit identity and scope

- Task path: `/root/impact_authorization_audit`
- Assigned model: `gpt-5.6-sol`
- Assigned reasoning effort: `xhigh`
- Mode: read-only architecture/security/provenance audit
- Repository worktree: `/home/doll/Project-Peritus/.worktrees/formal-coverage`
- Branch: `feature/formal-coverage`
- Pull request: draft PR #74, head `feature/formal-coverage`, base `develop`
- Snapshot date: 2026-09-13 (America/Chicago)

This audit did not edit repository files, run builds or gates, change GitHub settings, approve a proof-change record, register an actor, or create review evidence. The assigned model and effort above are the parent task's spawn metadata; they are not being offered as a proof-impact reviewer attestation.

## Frozen objects reviewed

The review was anchored to immutable Git objects rather than the mutable checkout:

- Protected-base snapshot `B`: `8e8cbb1bcf0de9787d86c7be3d6fc1f2079f3493`
- Base tree: `3e5febb6c672bf24e86a1c8823f2e931fb33aef1`
- Candidate snapshot `C`: `d0e4f0cf0943c7194adaaeabcb11fc70badc3fe8`
- Candidate tree: `73791a6d6079fd026cf6595fdc37dcf565456212`

Both commits verified locally as commits carrying good Git signatures by the user's configured key. That establishes commit-signature evidence only; it does not substitute for the independent proof-impact review required by the repository contract.

The mutable checkout acquired concurrent, unstaged parent-lane changes after this audit began. All reviewed hashes below therefore refer to `git show C:<path>` bytes, not current working-tree bytes. The protected-base reference is also a time-sensitive snapshot: if `origin/develop` moves before review, `B` must be refrozen and every base-bound artifact recomputed.

## Current source and obligation state

- `verification/proof-impact.toml` is byte-identical at `B` and `C`: SHA-256 `39135a20188b9415102c51c04e44f53fad27ea73a0775bbb4623a251b88a6d44`.
- It declares 3,390 source records and five PCR records, `PCR-0001` through `PCR-0005`, all with status `approved`.
- Its source snapshots do not describe either live tree. Against the manifest's declared source hashes, `B` has 168 actual-byte drifts. `C` has 361 actual-byte drifts/transitions, including the removed path `crates/app/peritus-product-runner/src/verified_api/progress.rs`.
- The existing protected-trust log stops on that removed path with `PERITUS-XTASK-IO-001`. This is the first hard I/O failure, not an exhaustive report of every mismatch.
- `verification/obligations.toml` has 153 obligations. Every one is `in-progress`; none carries `reviewer`, `review_date`, or `exclusion_id`. The 16 branch-added entries, `OBL-0209` through `OBL-0224`, are also `in-progress`.
- `architecture.toml` currently assigns a formal class to 66 packages: 53 class `H`, 12 class `V`, and one class `T`. `peritus-local-socket` is a new class-`H` package relative to the 65-package affected sets recorded by `PCR-0005`.
- Shared trust inputs affect all 66 formal packages. The shared set is `.cargo/config.toml`, `Cargo.lock`, `Cargo.toml`, `architecture.toml`, `rust-toolchain.toml`, `toolchains.toml`, `verification/actor-provenance.json`, `verification/actors.toml`, `verification/exclusions.toml`, `verification/obligations.toml`, and `verification/trust.toml`.

Nothing in this snapshot truthfully supports marking any current obligation `proved`, `reviewed`, `excluded`, or otherwise complete. The truthful status remains `in-progress` until obligation-specific evidence and the ledger's own review rules are satisfied.

## Root causes in the checker and schema

1. **The manifest is historical-record complete but actual-tree stale.** `manifest_impact.rs` checks a strict envelope, an ordered PCR chain, conservative change-kind coverage, exact expected inventory, actual current bytes, and affected packages. The unchanged manifest cannot describe the 361 current candidate transitions.

2. **Inherited base drift is real.** The protected base already has 168 actual-byte mismatches, so it cannot pass a normal current-tree trust check merely because it is protected. The candidate overlay exists to reconcile inherited debt under a full exact-candidate review.

3. **`history_is_applied` is weaker than actual-byte trust.** In `candidate_inventory.rs`, `history_is_applied` reconstructs the declared `sources` array by applying the PCR record prefix and compares that reconstructed declaration with the manifest declaration. It does not hash the base tree and compare historical records with the base's actual source bytes. Thus the record-prefix condition can succeed while the base still has 168 actual-byte drifts.

4. **There is no pending PCR state.** `ProofImpactStatus` accepts only `approved` and `revoked`. A deterministic reconciliation proposal can be emitted as an audit artifact, but it cannot be appended to canonical history as a synthetic `PCR-0006` with a made-up pending status. An appended PCR also cannot truthfully say `approved` until its genuine review, evidence, verdict, and gate conditions exist.

5. **Authorization and implementation are intentionally split.** `review_base.rs` requires an implementation transition to use an approved PCR already present unchanged in its protected base. A single PR cannot both append the approving PCR and apply the reviewed source bytes. `authorization.rs` permits exactly one appended PCR at the authorization tip and requires `current.sources == base.sources`; the final authorization snapshot may change only the allowed checker/test/review-document paths and must restore application/formal/shared/actor/provenance inputs to base bytes.

6. **The current PR head is not an approvable implementation tip.** `C` has no `PCR-0006` in protected-base history, no detached PCR-0006 verdict, no authentic fresh reviewer record for this review, and no exact-C gate evidence. It may serve as input to a future frozen candidate only after all final candidate bytes and authentic actor provenance are settled; any byte change creates a new `C` and tree hash.

7. **Protected hosted authority is not established by this branch.** `.github/workflows/formal-authority.yml` exists at `C` (SHA-256 `18c818b0d650413fc407377293221018c0ac151a6e8ea4fbc182a5fb00e6601f`) but is absent from `B`. The workflow also requires authority inputs to match both checker-to-base and base-to-candidate comparisons. PR #74 changes authority inputs, including workflows, `Cargo.lock`, and checker sources, so it cannot supply its own protected checker/authority. A local `verify-trust` success would be diagnostic evidence, not protected authorization. This is the separate GAP-07 bootstrap blocker.

## Authentic actor and verdict provenance required

The protected registry currently has four actors: `ACTOR-0001` and `ACTOR-0003` have the owner role; `ACTOR-0002` and `ACTOR-0004` have the reviewer role. Those protected reviewer identities cannot be reused for a new authorization.

For the eventual candidate and verdict:

- Always enroll a genuinely fresh reviewer identity derived from an actual read-only review run. Do not reserve an `ACTOR-*` identifier or fabricate provenance before that run exists.
- The reviewer provenance must name the actual Codex collaboration principal, task, session/locator, model `gpt-5.6-sol`, effort `xhigh`, and mode `read-only-review` exactly as required by `manifest_actor.rs` and `candidate_actors.rs`.
- The owner and reviewer must be distinct actors and distinct principals. An existing owner may be retained only when it is the same real durable identity; otherwise enroll the actual owner too.
- Candidate actor/provenance additions must be present in the exact implementation tree being reviewed. Protected actor/provenance records remain semantically preserved; an actor row may change only for the checker-defined whole-provenance digest refresh.
- Because actor/provenance files are shared source inputs, their final bytes belong in the candidate inventory and affect all 66 formal packages. The inventory must be regenerated after these identities are finalized.
- The detached verdict must bind the exact PCR id, reviewer and principal, full UTC review time, `B`, implementation commit `C`, implementation tree, source digest, gate digest, finding digest, and artifact digest.
- An `approved` verdict requires every canonical gate to have passed and no unresolved blocking finding. The review report and complete finding/artifact ledgers must exist and be hashed. Commit signatures and a parent-agent audit cannot replace this evidence.

This task is an audit, not that independent PCR review. It grants no permission to cite `/root/impact_authorization_audit` as the eventual reviewer or to construct a reviewer record from this report.

## Exact legitimate reconciliation path

1. Keep the current export as an audit-only, deterministic pending proposal outside canonical PCR history. Do not append `PCR-0006`, mark it approved, register a fake reviewer, or claim gates passed.

2. Immediately before the real review, fetch and freeze the protected base:

   ```bash
   git fetch origin develop
   B=$(git rev-parse 'origin/develop^{commit}')
   ```

   If this no longer resolves to `8e8cbb1bcf0de9787d86c7be3d6fc1f2079f3493`, reconstruct the candidate from the new protected base and recompute all base-bound digests and transitions.

3. Create a frozen candidate `C` descending from `B` that contains all final implementation/formal/shared bytes, the final truthful 153-entry obligation ledger, and authentic actor/provenance entries obtained from the actual owner/reviewer sessions. Record both immutable identifiers:

   ```bash
   C=$(git rev-parse 'HEAD^{commit}')
   C_TREE=$(git rev-parse "$C^{tree}")
   git merge-base --is-ancestor "$B" "$C"
   ```

4. Recompute the candidate inventory through the checker's normal discovery, architecture, and Cargo-metadata paths. Each `previous` snapshot comes from the protected manifest record; each `current` snapshot comes from exact `C`. Removed files use an absent current value. The current 361-transition result is a snapshot for `d0e4f0cf...`; recompute after every final source, obligation, actor, or provenance change. Preserve `PCR-0001` through `PCR-0005` byte-for-byte and in order.

5. Run the complete canonical gate set against exact `C` and retain raw outputs. The contract requires two evidence rows for every affected package, so the all-shared-input reconciliation requires 132 rows: 66 ordinary tests and 66 class-correct Verus checks. The command forms are:

   ```bash
   cargo test --package <package> --all-targets --all-features --locked
   cargo verus verify --package <package> --all-features --locked --check-toolchain --fwd-verus-args-to roots -- --no-cheating
   ```

   The `--no-cheating` argument is required for classes `H` and `V` and omitted for class `T`. Use the exact command normalization emitted/accepted by `evidence.rs`, including any repository-required resource-limit argument. A blanket `cargo xtask all` does not perform the protected proof-impact authorization check and is insufficient.

6. Have the fresh reviewer independently inspect exact `C`, the exhaustive transition/affected-package set, raw gate outputs, obligations, findings, and artifacts. Only that reviewer may author the real report, finding ledger, detached verdict, and approval decision. Until then the proposal stays pending outside the schema.

7. Construct authorization tip `A`. It may descend from `C`, but its final snapshot must restore every application/formal/shared/actor/provenance input to `B`. It retains the exact reviewed checker/test changes permitted by `authorization.rs` plus allowed docs, `verification/proof-impact.toml` with exactly one appended approved PCR, `verification/reviews/PCR-0006.toml`, and the genuine review artifacts. Its top-level `sources` array must remain byte-equal to `B`'s array. The verdict binds `authorization_base_commit = B`, `implementation_commit = C`, and `implementation_tree = C_TREE`.

8. Validate `A` with the checker that has already become trusted through the separately controlled GAP-07 bootstrap. The hosted form requires the full, nonzero base commit:

   ```bash
   GITHUB_ACTIONS=true PERITUS_PROOF_IMPACT_BASE="$B" cargo xtask verify-trust
   ```

   This command is illustrative until GAP-07 establishes that the invoked checker/workflow is the protected authority.

9. Merge authorization tip `A` first. From protected `A`, create a separate implementation tip `I` that reapplies the exact reviewed `C` application/formal/shared/actor/provenance bytes while retaining the approved PCR and verdict. Validate with `PERITUS_PROOF_IMPACT_BASE=A`. The candidate commit and tree must remain reachable in published history so a fresh clone can resolve and recheck every binding.

PR #74 at `d0e4f0cf...` therefore cannot be directly approved or merged as the implementation transition. It can be reorganized into this two-stage history only after a final candidate with authentic identities, exhaustive transitions, gates, report, and verdict exists; the application requires a subsequent PR after the authorization tip is protected.

## What is presently approved, pending, and open

- **Recorded approved history:** only existing `PCR-0001` through `PCR-0005`, as immutable protected-prefix records. This audit does not reissue or extend their approvals.
- **Pending audit proposal:** reconciliation of the current 361-transition candidate inventory and 66-package affected sets. It is suitable for deterministic review input, not canonical approval.
- **Obligations:** all 153 remain `in-progress`.
- **PCR-0006:** nonexistent and unapproved. It must stay nonexistent until genuine evidence and an approvable detached verdict are ready.
- **Current PR:** draft and non-authoritative for proof impact at the reviewed snapshot; no authentic PCR-0006 review or verdict was present.
- **Other checkpoint gaps:** GAP-03 through GAP-08 remain open. GAP-07 specifically blocks protected checker/workflow authority; this audit does not broaden into resolving them.

## Reviewed immutable-path hashes

All hashes are SHA-256 of `git show d0e4f0cf0943c7194adaaeabcb11fc70badc3fe8:<path>` bytes:

```text
b43a9d7b7ea45cbfdf7eedb2db0b10edd976dde37039cd647628a381d59e3e33  xtask/src/trust/manifest_impact.rs
093c1570542c70a65da96fcde4bc8d80ed3181a7606722f577eb10d217aa4b23  xtask/src/trust/manifest_impact/authorization.rs
e519c777c81212b0cd911b12df13eb8998aa875310deb8019576602588d48f95  xtask/src/trust/manifest_impact/checker_binding.rs
f91d3ce3c38afeaa6dbe2146ec36bbc18f642e2ad5500b6155cbcb03fb0ce871  xtask/src/trust/manifest_impact/review_base.rs
06de7f677aab7f6e14e9e2e2cc6e28ec4ad093d78e495f0da79795952e7341cd  xtask/src/trust/manifest_impact/candidate_actors.rs
596d257f28b048fb79c7f632334683d7192ac12647d74558d7647e6ff7712596  xtask/src/trust/manifest_impact/candidate_inventory.rs
7d3dc9697346fde60ee6f3935fae922d75b3a22162459ad7fe30b0ac734faa5a  xtask/src/trust/manifest_impact/evidence.rs
95d434de640b2d41e2e2d677f72e4e939f25014d053dce3afad8b21fad68b7d7  xtask/src/trust/manifest_impact/inventory.rs
1143582a85c9c2b9d600cc4bee37c1d131a2e25f975f46d738fbfe01b1c62271  xtask/src/trust/manifest_impact/verdict.rs
851a814b0af00b13588ca872febe1f4f714f83285802dd8835b97ef128e75bb2  xtask/src/trust/manifest_impact/verdict/git.rs
70a22e7bf3e6dab11ce24574d7013cb38dfc8e824a9a1cc6d5a85e3e8acef273  xtask/src/trust/manifest_impact/verdict/artifact.rs
936eee248f48393412f3d3533e32436352d0ad0f3c06a92b719e08bc5ee0a31f  xtask/src/trust/manifest_impact/verdict/digest.rs
ac66efa582e9fe353b1a094e7f965ee626d50050b0d3fb366105fee7d61495d2  xtask/src/trust/manifest_impact/verdict/directory.rs
8824546eacaca79721b16e1b851040e84c82c53cd9e34d3f17fd6dcb0cbb49cc  xtask/src/trust/manifest_actor.rs
043ee5420fd206d722aa630838fe2ca48e9d5a60e731e6e4dea7b93fa50bb5dc  xtask/src/trust/manifest_actor_model.rs
7e4e8e0a7412220aa22bba65c6e05edf771131096227adc7d6c56cdb81cb9e97  verification/README.md
39135a20188b9415102c51c04e44f53fad27ea73a0775bbb4623a251b88a6d44  verification/proof-impact.toml
2fc6645560d1d95118d5cd9f08260953316ede9971e146ef06103f8b9462dad2  verification/actors.toml
fe8559d6c7018bb444007a772f154946086f066c4983a92e60b87374f21157a6  verification/actor-provenance.json
6796854430d100cee4a8440f786d995f55540461cf285dbcae806d5fe0a02aeb  verification/obligations.toml
799985c930fc7f8d6cbe12770ca0d0bc85cbd182b458d4cd269fffbba1d6b1ac  docs/formal-coverage-checkpoint.md
4a2b3f206cd74a87c43a7d4d3697148df72717eee5863232449907139bc2979a  docs/formal-method-mapping-fix.md
59e175c4076cf4cd5bfe75ba041ec98d370462b55beb63d2b4aa9285acf0ad63  docs/formal-coverage-evidence/method-mapping/protected-trust.log.txt
18c818b0d650413fc407377293221018c0ac151a6e8ea4fbc182a5fb00e6601f  .github/workflows/formal-authority.yml
```

## Audit verdict

The legitimate deliverable now is an **audit-only pending reconciliation artifact**. It can enumerate exact source transitions and affected packages for a future review, but it cannot approve `PCR-0006`, discharge obligations, register a reviewer, or establish protected authority. Approval becomes truthful only after a final immutable candidate, authentic fresh reviewer provenance, 132 canonical passing gate records with raw outputs, complete report/findings/artifacts, an approved detached verdict, and the two-stage authorization-then-implementation history. GAP-07 must separately establish the trusted hosted checker before a local validation can count as protected authorization.
