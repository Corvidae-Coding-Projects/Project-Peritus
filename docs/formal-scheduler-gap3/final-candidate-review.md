# Independent final-candidate technical review

Reviewer: `/root/gap3_final_candidate_review`, an independent read-only subagent. This is a technical review artifact. It is not human approval, a GitHub review, protected-branch authorization, or permission to merge.

## Reviewer execution provenance

The parent orchestration requested model `gpt-5.6-sol` with `xhigh` reasoning effort for this review. The reviewer worker reported that its runtime identified itself as GPT-6 and that it could not introspect the launch effort. This record preserves both observations and does not claim an actual model or effort beyond what each side reported.

## Exact review boundary

- Comparison base commit: `a41113d1389c3d8fc77a00cc562cb9a5bcb7adc8`.
- Comparison base tree: `2902edba7095a6ebd53b02bbf6f19005193d6deb`.
- Reviewed candidate commit: `5d739de2902de0a5379f23f32b0c9f0ae562b28c`.
- Reviewed candidate tree: `3ca8f9e7fa5388ae3baa127c51c4bb5bd5c4fed7`.
- Comparison: the complete 324-path `base..candidate` diff.
- Scheduler source freeze: [`257-final-scheduler-source.sha256`](evidence/257-final-scheduler-source.sha256), containing 189 scheduler source and test paths.
- Scheduler source manifest SHA-256: `fe6b85f563ed3cbc6ca990b3e6e6d041734eb2592da0ba4a23416b911bd67240`.

The full commit and tree identities bind every reviewed path, including callers, protocol/schema material, generated fixtures, proof tooling, documentation, and retained evidence. The narrower source manifest independently binds the scheduler files exercised by the strict scheduler proof and runtime gates.

## Verdict

**PASS for the exact candidate commit and tree above.** The reviewer found no concrete correctness, specification, production-integration, compatibility, proof-trust, or maintainability blocker in the reviewed comparison.

The final candidate closes the identified production-code GAP-03 proof gaps for dependency propagation, exact selection and dispatch, pending directives, and the `start`/`decide` commitment seams while preserving the behavior and error ordering of the existing executable scheduler. The earlier admission, cancellation, worker-loss, phase, terminal, replay-reconstruction, durability, wire-compatibility, and caller increments remain coherently integrated in the same candidate tree.

No open finding remains within this technical review boundary. Delivery gates outside this review are listed below and must not be inferred from this PASS.

## Technical assessment

Dependency propagation applies each dependency round from one immutable snapshot and carries exact action traces, retained identity layout, state framing, collection ordering, and queue preservation. Its natural measure assigns two units to dependency-waiting work, one to queued work, and zero to other phases. The finite two-step-per-record budget strictly decreases across successful rounds, and a valid state cannot exhaust that budget before reaching the declared dependency fixed point. Production `refresh` invokes this verified propagation before refreshing worker phases.

Selection proves finite feasibility scanning, the first feasible worker in retained worker order, and the exact work ordering: aged status, descending priority, ascending enqueue ordinal, then work identity. A full active-reservation collection still rejects before selection or capacity scanning, preserving the established rejection precedence and early-exit behavior. A successful result is tied to the exact selected work and worker; absence is tied to the absence of any feasible candidate.

Dispatch uses the production typed adapter, including the scheduler-phase rejection. Its contract preserves the complete ordered rejection sequence and binds a successful event to the exact selected work, selected worker, dispatch identity, and dispatch token. The ordinary error adapter retains the pre-existing `SchedulerErrorKind` values and detail strings.

Pending directive construction is the exact canonical reservation-order filter/map truncated at the configured batch cutoff. It emits `Dispatch` only for an unstarted reservation whose work is `Reserved`, and `Cancel` only for a reservation whose work is `Cancelling`. Each directive remains bound to the complete source reservation identity.

Production `start` calls the verified genesis preparation and commitment seams. Production `decide` calls the verified cursor preparation and event commitment seams. These changes retain the original validation, application, encoded-size check, cursor update, digest computation, and event construction order, with one successor hash computation. SHA-256 execution, encoded-state size evaluation, and the ordinary application boundary remain explicit rather than being represented by unsupported proof claims.

Replay continues to reconstruct causative commands, reduce through the same production paths, and compare the complete reconstructed event. Its standard-library set membership and event equality remain executable boundaries covered by deterministic replay, tampered-history rejection, durability, restart, and session-caller tests. The reviewer found no need to add a separate proof model for those standard-library operations.

The reviewer checked that the proof obligations are connected to production-called functions and that success, rejection, no-op, preservation, and termination claims are nonvacuous under the scheduler's stated ready and ordered invariants. No newly introduced `assume`, `admit`, `external_body`, axiom, proof stub, `todo!`, `unimplemented!`, or public API compatibility break was found in the reviewed comparison.

## Withdrawn review concern

The reviewer initially flagged `#[allow(clippy::too_many_arguments)]` in `src/command.rs` as a possible new suppression. Exact base/candidate comparison showed that the suppression already existed in the base; the scheduler contains 14 `#[allow` sites at both the base and candidate commits. The line moved as surrounding command fields changed, which made it appear new in a broad diff view. The reviewer withdrew the concern. It is recorded here so it is not carried forward as an unresolved finding.

## Evidence inspected

The reviewer inspected the retained artifacts produced by the root validation lane and did not run an independent Cargo or Verus lane.

| Artifact | Observed result | SHA-256 |
|---|---|---|
| `257-final-scheduler-source.sha256` | 189-path scheduler source freeze; native `sha256sum --check --quiet` exited 0 | `fe6b85f563ed3cbc6ca990b3e6e6d041734eb2592da0ba4a23416b911bd67240` |
| `251-final-combined-verus.log` | Strict scheduler verification: 693 verified, 0 errors, with `--no-cheating --rlimit 20` | `23de64eb82001c2a698cc9363d70af05412365fb23b40825d60f61c1652e13e1` |
| `252-final-scheduler-tests.log` | All 79 scheduler tests passed; 0 failed and 0 ignored | `66ce70bbb06295d88876231394310b7e300da862f140f58b54fb5f1ea7f4ce49` |
| `254-final-scheduler-clippy.log` | All-targets, all-features scheduler Clippy passed with warnings denied | `af633a047ce8f58641feabec697a7a94720058421ab8b2b122e6dec9021e5b55` |
| `253-final-format-check.log` | Workspace formatting passed; successful command produced an empty log | `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855` |
| `255-final-architecture-check.log` | Architecture check passed: 84 packages and 4,567 source files | `9344fecc38e9b1cd097e9258469b652ce55a026a65751b9e7c138a7d26382c0c` |
| `256-final-ordinary-api-check.log` | Ordinary API check passed: 3,654 formal-boundary files and 14,765 ordinary-safe executable entry points | `a888edcbe7f661d6671354d5256cbeda45892f7a09b59bce252bc973cbfbd0b5` |
| `258-final-docs-check.log` | Documentation check passed: 257 documentation files | `ecb264e467123c85b60e854e089da101524da78fa0275caab28b8f6aebb0a30e` |

## Limits and remaining delivery gates

This review is bound to the exact candidate source. Any later source change requires a new candidate identity and renewed review. Adding this review record and later generated trust/evidence records does not alter the reviewed source commit, but those records must bind back to the commit and tree above.

This PASS does not report completion of the 66 ordinary package commands or 66 full Verus package commands. Their retained outputs remain separate delivery evidence. It also does not report current-head hosted workflow success; queued, running, cancelled, or superseded jobs are not passing jobs.

Proof-impact fingerprints, affected-package inventory, actor and review provenance, obligation status, authorization/application sequencing, and protected-base `verify-trust` remain separate reconciliation requirements. Historical approvals must be preserved. This review does not create or impersonate a human reviewer, a GitHub approval, or a protected-base authorization.

The remaining explicit ordinary boundaries are SHA-256 execution, encoded-size evaluation, standard-library replay membership/equality, persistence effects, and external worker progress. They are identified boundaries with executable coverage; this review does not relabel them as Verus proofs.
