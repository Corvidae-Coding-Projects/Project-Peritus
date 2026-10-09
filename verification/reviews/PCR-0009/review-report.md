# PCR-0009 independent proof-impact review

PCR: PCR-0009
Reviewer: ACTOR-0010
Reviewer principal: Corvidae-Coding-Projects/Project-Peritus/session/102/task/root/batch3_formal_review_sol
Decision: approved
Reviewed at: 2026-10-09T05:29:24.810885Z
Candidate: 2353e5dffc12430cfdc727b61cabcb13d3eda984 00f6c888ea957a669a7e39706bf7200282161391
Authorization base: 90925794845121dd9278d06c5eec92a5bbfd85e9 650ce4a59476178da8bd9efbcafc1ef1c7419938

## Review boundary

This is an independent, source-bound review of the Batch 3 minimum-repair candidate. I did not author the candidate source or the canonical proof register. I read the proof-impact protocol, provenance requirements, selected-scope reports, implementation and recovery paths, repair diffs, focused regressions, and the candidate reconciliation. I reproduced high-risk failures through public APIs outside the repository where practical and rejected precursor candidates when the evidence disclosed defects.

The reviewed candidate is the signed commit above, pushed at `origin/fix/minimum-repair`, descending from the exact authorization base. The final `fbc52be98e05428283b3d8bcefc20d230d65b336`-to-candidate delta changes exactly five test files. It binds a restart-output assertion to the persisted match offset instead of the variable libtest prefix, distinguishes discovery omissions from search's additional binary-content omission, makes the Git paging fixture platform-bounded while proving continuation for its exact long entry, and makes terminal prompt observation span bounded event pages and split output events. It changes no production code; all earlier functional repairs remain byte-identical to the previously reviewed tree.

## Source and behavior coverage

The review covered the selected Batch 3 repairs for native path representation and directory inspection; output retention and recovery; structured command/environment admission; attachments, evidence budgeting, media discovery, and archive identity; optional preview deadlines and graphical capture; full-output behavior evidence and goal qualification; structured review paging, parsing, raw access, and TUI navigation; Git diff/history observation and immutable identity; mutation publication/recovery; tool registration; protocol history/media paging; and formal obligation/trust/provenance changes.

I traced implementation, callers, wire contracts, persistence, recovery, and final publication paths. Focused checks included exact archive restart identity; attachment range identity; process ownership and cancellation; checkpoint cursor and recovery fencing; Git candidate/rollback receipt reconciliation; bounded diff parsing and raw fallback; Git history identity under repository configuration; process spool failure; active PTY/pipe evidence; no-deadline capture authority; and durable graphical-goal evidence across crash, corruption, retry, and goal edits.

Twenty-eight blocking findings arose during review. Every finding is recorded in the candidate-bound ledger with separate detail and evidence. Precursor candidate `6478b2ab02e1d66367e16758e459804f44845f2a` was rejected for functional defects. Precursor candidate `b8eee4bd666bd51b84c824cc131b9f82959cd62e` was superseded after hosted strict Verus CI found a qualification lint. Candidate `fbc52be98e05428283b3d8bcefc20d230d65b336` was superseded after the exact formal campaign exposed a runner-dependent daemon assertion at gate 17 and hosted macOS CI exposed an operation-specific filesystem expectation. Candidate `0e6736b0a64ca81fb73c6071f98b39f24288c993` was superseded after hosted macOS runtime-tools CI exposed an overlong Git paging fixture and review found that its generic continuation flag did not identify the intended path. Candidate `a265625ecbc956010362bee4d0b4e3108cb48ade` was superseded after hosted macOS daemon CI exposed a terminal prompt test that assumed one bounded event page and one event contained the full marker. No result from a precursor campaign is reused.

## Proof-impact reconciliation

The fresh reconciliation bytes have SHA-256 `c5a4116eaff5ec5699a98e65ab7960f80546a759fbf600acc4e60963a0f8aa8c`. I independently verified every raw Git blob represented by all 3,993 candidate and 3,904 protected inventory sources, the inventory content hashes, 548 branch transitions, 667 inherited-record drift entries, their sorted 1,025-transition union and chains, and the exact 66-package/132-command plan.

The reconciliation preserves PCR-0001 through PCR-0008 and all 562 referenced historical artifacts. All 157 obligations remain unique and `in-progress`; none is removed or discharged. The ACTOR-0010 enrollment matches this reviewer principal, session 102, issue 103, model and reasoning record, while explicitly remaining self-reported, audit-only provenance. At reconciliation time, the four recorded remaining requirements correctly deferred gate evidence, authorization-only registration, source application, protected-base rerun, deployed checker authority, and hosted CI qualification.

## Exact qualification evidence

The fresh campaign for this exact candidate/tree completed all 132 canonical commands in plan order from `2026-10-09T03:56:11.159449Z` through `2026-10-09T04:10:35.069589Z`. Independent reconstruction and audit verified 132 distinct passing results and raw logs, 132 unique log hashes, exact state/header/footer/command/invocation bindings, the reviewed plan and driver, required environment/timeout/CPU controls, serial timestamps, zero source-integrity errors, the exact 134-file campaign directory, and a clean unchanged checkout. The independent audit evidence has SHA-256 `413eaaf031311e42a3f72a860afceff6090465dc13ca7e8e4e8a853ddfa5940a`.

The exact-candidate ignored native graphical pair passed 2/2 in 30.62 seconds with a 60.046-second command elapsed time; its raw log has SHA-256 `f20ebf08d4c123a03523700737de031d437289515b512cba200216e6dfcf8679` and its independent audit evidence has SHA-256 `7ad6a977c7537290a1db80a4871979b36f611e33f31da47f5df51e8fd7ca9047`. Exact-candidate H0 native qualification passed on Linux, macOS, and Windows in run 37881442451. The exact-candidate Foundation matrix passed all 194 jobs in run 37881442343, including macOS runtime-tools job 113661964151 and macOS daemon job 113661969772. The retained Foundation and H0 snapshots have SHA-256 `dcea6c41dba39becb58a395c09fb61eb6bc783db2ceafd01d8a2c521f33e088c` and `2b3f14c85ce550f9162c4034f27f46b9610d8bff401a7de1781448e29cfa62d3`; the independent hosted qualification audit evidence has SHA-256 `9a64a75f4d8a3eb4b102d4ca1cd1102beedea9af2b5e1516b271e11bbc403faf`.

## Limits and remaining authority

This review is scoped to the selected Batch 3 repair set and the exact source tree; it is not an exhaustive certification of every repository behavior. The live `develop` base is immutable by commit identity for this review but is not covered by an active GitHub branch-protection rule; GAP-07 therefore remains. This source verdict cannot claim protected or deployed checker authority, cannot merge either phase, and cannot discharge historical obligations. Authorization-only registration and exact application remain separate trees and must be independently checked against their exact bases.

## Verdict

I approve the exact candidate `2353e5dffc12430cfdc727b61cabcb13d3eda984` with tree `00f6c888ea957a669a7e39706bf7200282161391` for PCR-0009 authorization-only registration and subsequent exact source application through separately audited commits. This verdict is bound to the reconciliation, finding ledger, gate campaign, native qualification, and hosted qualification identified above. It does not merge either phase, discharge any of the 157 in-progress obligations, establish protected or deployed checker authority, or extend to a different source tree.
