# PCR-0011 independent source and proof-impact review

PCR: PCR-0011
Reviewer: ACTOR-0012
Reviewer principal: Corvidae-Coding-Projects/Project-Peritus/session/102/task/root/final23_formal_review_sol
Decision: approved
Reviewed at: 2026-10-09T23:21:43Z
Candidate: 6889f8c5dcca335b71c572865d5483c8e68c0890 8f952188d114ce3d2446fa085a84b95cfd57b143
Authorization base: 96eec678dc49b7ac4c62b39bf57c00c5b0b70d5b 680504fbe3a666fe3fa0bc0af462f0fedd00f2af

## Review boundary

This is an independent, read-only review of the exact final23 implementation candidate and its complete proof-impact transition. I did not author the candidate, repository checker, obligation ledger, reconciliation program, or gate driver. I inspected the protocol and actor contracts, candidate source, callers and recovery paths, independent delegate ledgers, every uncovered or stale-ledger path, the exact reconciliation, raw ordinary and Verus gate records, native probe artifacts, hosted platform results, and the protected-input history. Candidate commit and tree were rechecked before and after each formal gate.

After completing the source and gate review, I reviewed and hardened the temporary outside-repository proof-artifact builder and wrote or edited its independent artifact auditor, review finalizer, signed phase-object constructor, previous-tip checker runner, and authority-package generator. Those helpers only validate, retain, or package independently reviewed inputs; they do not modify the candidate or repository checker, infer this decision, replace the evidence review, or authorize an external mutation.

The reviewed commit is signed, pushed as the exact head of `origin/fix/minimum-repair`, and descends from the source authorization base. Its source tree remains frozen. The live GitHub `main` and `develop` tips remain `90925794845121dd9278d06c5eec92a5bbfd85e9`; they do not yet contain the protected PCR-0009/PCR-0010 chain or PCR-0011 authorization. This report is therefore a source verdict for construction of separately checked A11 and I11 commits. It is not a claim that protected authority is current, that PR 113 is mergeable, or that any branch or ruleset has been changed.

## Source coverage and behavior

The source-base-to-candidate delta has 383 paths. I rehashed the exact final blobs for all 382 paths covered by the application and native review ledgers and directly reviewed the remaining `xtask/src/discovery.rs` registry change. The resulting coverage ledger has SHA-256 `95183c50adfbbed43e57f61ea126a72a472ef83707f11e751772b234c3fbbf2d`; the direct final supplement has SHA-256 `286932db9cbb3639dd0ee824d518c4d0e89418e5a1329d0be5bc39e1569a0f0b`. Split-module token comparisons established that the layout repair moved the reviewed implementation without changing its behavior except for required private visibility and formatting normalization.

The review traced continuation admission and durable custody; authenticated instruction expansion; immutable attachments and version-bound range reads; project-root-bound directory inventories; nested Git registration; terminal input and output retention; process spool ownership; updater extraction, installation, and descendant cleanup; Windows sandbox preparation and teardown; ACL identity, transactional rollback, inheritance, and residual-authority decoding; native evidence publication; lifecycle discovery; and their production callers and failure paths. The exact candidate resolves every blocking instance found during review. Three pre-existing native cleanup gaps remain explicitly open and outside the changed final23 promises: helper-manifest failure after native readiness, consumed Windows proxy teardown ownership, and detached restricted-helper ConPTY forwarding tasks.

## Complete finding ledger

| ID | Severity | Blocking | Disposition | Subject |
|---|---:|:---:|:---:|---|
| FINDING-0001 | high | yes | fixed | Continuation could launch a different mode or input revision |
| FINDING-0002 | high | yes | fixed | Long instructions changed provider and tool read authority |
| FINDING-0003 | high | yes | fixed | Nested Git could copy dirty child bytes into the parent |
| FINDING-0004 | high | yes | fixed | Updater extraction and installation ownership ended early |
| FINDING-0005 | medium | yes | fixed | First frozen candidate violated the 500-line source budget |
| FINDING-0006 | high | yes | fixed | Windows evidence idempotence flushed a read-only handle |
| FINDING-0007 | high | yes | fixed | ConPTY inherited redirected runner standard handles |
| FINDING-0008 | low | yes | fixed | Unix-only mutation failed strict Windows lint |
| FINDING-0009 | high | yes | fixed | PTY tail reconstruction exceeded the real deadline |
| FINDING-0010 | high | yes | fixed | PowerShell 5.1 coerced a null replacement backup path |
| FINDING-0011 | high | yes | fixed | Updater root success could abandon a Job descendant |
| FINDING-0012 | medium | no | open | Helper-manifest failure can lose native child custody |
| FINDING-0013 | medium | no | open | Windows proxy teardown consumes its retry owner |
| FINDING-0014 | medium | no | open | Restricted-helper ConPTY forwarding tasks are detached |
| FINDING-0015 | high | yes | fixed | Aliased protected roots could evade ACL projection |
| FINDING-0016 | high | yes | fixed | Directory inventories were not project-root-bound |
| FINDING-0017 | medium | yes | fixed | Windows preview line submission used the wrong terminator |
| FINDING-0018 | low | yes | fixed | ACL oracle rejected an equivalent symbolic rights mask |
| FINDING-0019 | medium | yes | fixed | Completed macOS updater groups could report false failure |
| FINDING-0020 | low | yes | fixed | macOS cleanup repair failed target lint policy |
| FINDING-0021 | high | yes | fixed | ACL rollback lacked retained identity and process ownership |
| FINDING-0022 | high | yes | fixed | ACL replay could succeed before later propagation changed originals |
| FINDING-0023 | high | yes | fixed | Inherited ACEs were omitted from residual comparison |
| FINDING-0024 | high | yes | fixed | Process launch exposed streams before spool creation |
| FINDING-0025 | low | yes | fixed | WebUI regression assumed an unpromised release ordering |
| FINDING-0026 | high | yes | fixed | ACL propagation could merge authority from an outside ancestor |
| FINDING-0027 | medium | yes | fixed | Lifecycle discovery depended on anonymous Docker Hub capacity |
| FINDING-0028 | medium | no | open | Exact-head browser qualification had one unexplained selection timeout |

Each row has a separate retained detail and evidence artifact. The blocking findings are fixed in this exact tree. FINDING-0012 through FINDING-0014 are byte-identical inherited residuals and do not weaken the narrower final23 guarantees stated in their records. FINDING-0028 retains the first hosted browser failure and the unchanged-head repeats; it does not assign an unsupported cause or claim deterministic CI behavior.

## Proof-impact reconciliation

The independently audited reconciliation has SHA-256 `04bc49f756bad4b6784a52e4d8acbef645c746927e00d37e39b8cf173975a07e`. I verified all 4,053 candidate and 3,993 protected inventory entries as regular nonsymlink blobs with exact hashes. The 231 recorded transitions equal the complete inventory map difference, affect 66 packages, and yield the canonical 132 commands. PCR-0001 through PCR-0010 and all 886 historical review artifacts are preserved byte-for-byte. All 157 obligations remain unique and `in-progress`; none is removed or discharged. ACTOR-0012 is the sole candidate actor append and binds this reviewer principal, issue 103, session 102, read-only mode, `gpt-5.6-sol`, and `xhigh` effort. The reconciliation is deliberately `pending-authorization` and `audit-only`.

## Exact formal qualification

The exact candidate campaign ran all 66 ordinary-test commands and all 66 class-correct Cargo-Verus commands serially from `2026-10-09T21:15:28.049151Z` through `2026-10-09T21:26:00.476167Z`. All 132 passed with zero command or driver failures. Its plan SHA-256 is `875c746cf01fa6e0af94d49925a2c897772251ed7c698b8b48c0714ee499fae3`, state SHA-256 is `26f23b7d796b813f7b9afcbd1fa5aec68684d77f388e9c108af0320d3bce92a4`, driver SHA-256 is `7c966425b117c73ac99dec92f0d46f20b4379eab38f9e99f47a403c9d4371094`, and the 132-log set SHA-256 is `dbc2a6af07486e0550f4cc218909c30b1f89c04517daad9eb38af3c0fc27384a`. The campaign state remains explicitly `approval = false`.

I independently reconstructed every command and invocation from the exact candidate architecture, checked the separate plan bytes, environment, CPU affinity, timeout, strictly serial timestamps, 132 unique filenames and hashes, JSON headers and final result footers, exit values, and candidate freeze. The independent full audit log has SHA-256 `5a6b164cab3ede20f102e8ae868e16f5ff75ac4e512116116c1a8f7df9883e67`. No result from rejected or superseded candidates is reused.

## Native and hosted qualification

Exact-head Native security qualification run `37992279296` completed successfully on Linux, macOS, and Windows. The Windows shard passed all 16 native cases plus the complete conformance case, with zero surviving resources or cleanup failures. The independent summary has SHA-256 `3be86c02a7c4d135dc3662f308e9d8057056b9cee17cde5cc85c2269b756bf5b`; the raw Windows probe record has SHA-256 `cac1a911ebd15d2fe055a5ee0c1e8e3251f2ed4eea00b0badc869bc91a3880ba`. Native console isolation run `37992279284` and Proactive discovery run `37992279278` also completed successfully at the same head.

Windows Foundation runtime-tools job `114029945643` passed 319 tests with zero failures and zero ignored tests across 62 summaries. It includes both immediate pipe and terminal spool-startup regressions and all 16 native ACL tests. Its raw log has SHA-256 `c95b4f799e15c86cc9bf104d3993f5cf4a4e8f7f39f32414170b4b7730715a61`.

The remaining required exact-head workflows also completed successfully: Gate A run `37992279336` passed all 215 jobs; Native product package run `37992279410` passed all 162 jobs; and Foundation verification run `37992279493` passed all 194 jobs. The normalized API job records and exact workflow snapshots have summary SHA-256 `9c6f62f3df73493ab08e4bde3e023687672c87c9cd1e80095dacc41836b01cf7`. The Windows Gate A manifest has SHA-256 `232d4bb0db25f8ea0c6649d98de1004ccee6c2ff73dd875763299e12225822b3`; the independently checked 36-report Windows Native product scenario audit has SHA-256 `9077889b06069e599e6a0581ca4e563fdb936cdcece56cfaee91502b544d5cad`. These hosted passes qualify the exact candidate; they do not repair protected authority.

Browser workspace run `37992279524` failed its first attempt after 19 Playwright cases when the selected-session element did not appear within the existing five-second window; five later serial cases did not run. The failure log is retained with SHA-256 `d488bf31f344e99d506b1bb5926501df91cf96f9ffc54a4fc7b5a92f7c17ffc7`. Without any source or timeout change, attempt 2 passed all 25 Playwright cases and its Rust, lint, format, and build checks. Its pass summary has SHA-256 `5c3e9f03f5c44c5a0ce151a81972020072a1735269acaf2a564bd6e73cfcd304`; the separately archived raw API job log has SHA-256 `0b592f906726995f878aae58059d98183fb7ccc23b602ba176a08190c1fe079e`. One 15-case local workspace run and 75 cases across three full-suite repeats also passed; the gateway-restart case passed four times across those runs. Because no source root cause was established, FINDING-0028 remains open and nonblocking rather than being erased by the repeats.

## Protected inputs and remaining authority

The live-base-to-candidate protected-input delta is exactly 12 paths: `Cargo.lock`, ten `cfg(test)` xtask fixture files, and `xtask/src/discovery.rs`. Nine fixture files only update the layout test policy from soft 400/hard 700 to soft 500/hard 500; the source fixture also updates its generated lengths and diagnostic thresholds consistently. The discovery production change replaces only the Docker Hub registry prefix with Docker's official Amazon ECR Public mirror while retaining Alpine 3.22 and the exact immutable OCI index digest. Parsed lock records and normalized normal/build dependency trees show no xtask runtime dependency change. A fresh full xtask run passed 470 library tests with one intentional ignored fixture, four CLI tests, and four release tests. The protected-input audit has SHA-256 `2a82d924280e738837189900383ef26a8b54072958880bde6d2c73ec7bbfe085`.

The current protected GitHub base does not authorize these bytes. Formal authority correctly classifies the drift and skips trusted-base validation; that skip is not authorization. Historical PCR-0009 and PCR-0010 authorization/application tips, the identical source-base consolidation, and the future exact A11/I11 objects must be landed in their reviewed order through a separately controlled exact-tip maintenance procedure. This verdict does not authorize a ruleset edit, branch update, merge, force update, or PR retarget, and it does not claim that an ordinary merge commit preserves the reviewed identities.

## Verdict

I approve the exact source and proof-impact transition from the stated authorization base to candidate `6889f8c5dcca335b71c572865d5483c8e68c0890`. All blocking findings are fixed in this tree, the complete 132-gate campaign passed, and the required exact-head hosted and native qualification is complete. This approval authorizes construction of the separate PCR-0011 authorization and implementation objects described above. It does not authorize a branch or ruleset mutation, PR merge or retarget, force update, or obligation discharge, and it does not claim the live protected base already carries PCR-0009 through PCR-0011.
