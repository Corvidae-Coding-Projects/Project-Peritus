# Incremental-context replay waypoint

This folder operates production APIs to check reuse without changing source/permission semantics.
It is separate from Cargo tests. Build/relink after production changes; use disposable disk-backed
workspaces, keep independent expected values, and preserve actual failure inputs and results.

| File | Role |
| --- | --- |
| [cost-ledger-replay.py](cost-ledger-replay.py) | Real chat/review on/off pairs, failure/cancellation and accepted managed edit with independent canonical usage audit and content-free diagnostic checks. |
| [indexed-projections.py](indexed-projections.py) | Same-build reference/index comparisons across large retained histories, exact views and rejections, live read restrictions, fresh-process recovery, and optional actual-provider continuation. |
| [source-index-trace.py](source-index-trace.py) | Replays actual native C7 tool observations across index selections; rejects altered outcomes and sequence gaps on owned copies without changing committed roots. |
| [checkpoint-pages.py](checkpoint-pages.py) | Alternating matched checkpoint operations, canonical component/root checks, physical sealed-page write traces, and active-cache corruption replay. |
| [checkpoint-interruption.py](checkpoint-interruption.py) | Real debugger-controlled process kills at durability boundaries, SQLite publication failure, and fresh recovery without tool/provider redispatch. |
| [checkpoint-pages-live.py](checkpoint-pages-live.py) | Actual context updates and accepted managed CSV writing/gates/independent review with page reuse disabled and enabled. |
| [opencode-layout.py](opencode-layout.py) | Actual native layout/selection/correction and paired ProductRunner campaign with independent capture/usage reconciliation and strict daemon route checks. |
| [opencode-layout-consumer.rs](opencode-layout-consumer.rs) | Public C5 consumer using the full current host tool catalog, real selection controls and actual read-only workspace execution. |
| [opencode-layout-config.rs](opencode-layout-config.rs) | Public daemon route decoding/declaration consumer for defaults, rollback and invalid settings. |
| [opencode-layout-vendor.py](opencode-layout-vendor.py) | Transparent observer of the official native runtime; optional model-authored fault induction stays separate from savings. |
| [efficiency.py](efficiency.py) | Actual before/after context consumers, independent exact-source checks, model-view correspondence and checkpoint-work measurements. |
| [page-integrity.py](page-integrity.py) | Exact committed roots, fresh-process reuse, current-state rollback, and corruption rejection on owned state copies. |
| [plan-replay.py](plan-replay.py) | Independent dependent-read facts, exact result paging, current restrictions, restart continuation, v4 old-reader refusal, and execution-disabled rollback. |
| [plan-live-compare.py](plan-live-compare.py) | Comparable actual-provider search-to-read journeys with exact independent answers, unchanged files, per-request usage reconciliation, and alternate configuration order. |
| [plan-control-client.rs](plan-control-client.rs) | Negotiated production IPC client for exact goal, input and workspace-permission controls; creates only its owned public registry and folder fixture. |
| [plan-daemon-control.py](plan-daemon-control.py) | Actual native-provider daemon plans/waits under steering, permission changes, pause, cancellation and a full daemon crash/restart. |
| [plan-interruption.py](plan-interruption.py) | Actual kills before/after child publication and SQLite publication refusal, with exact committed-prefix retrieval and no recovery dispatch. |
| [command-wait-live.py](command-wait-live.py) | Actual native interactive output wait, exact Unicode stdin, and independently released terminal wait on one original handle. |
| [command-owner-consumer.rs](command-owner-consumer.rs) | Public C4/C2 owner consumer for actual native start, input, observation, cancellation and exact-owner recovery. |
| [command-owner-recovery.py](command-owner-recovery.py) | Running/terminal process kills, projection publication failure and foreign-owner rejection without provider inference or redispatch. |
| [input-invalidation.py](input-invalidation.py) | Changes live conversation revision under one memory owner; checks stale conclusion retention, unchanged pages, restart, and optional historical-reader continuation. |
| [decision-replay.py](decision-replay.py) | Exact compiler evidence, undeclared file changes, bounded descriptor/raw-byte paging, current restrictions, reviewer isolation and history rollback. |
| [decision-host-dependencies.py](decision-host-dependencies.py) | Real originating-view bindings, unknown inputs, model-boundary/graph rejection, v6 migration, publication failure and process-kill recovery. |
| [decision-ignored-input.py](decision-ignored-input.py) | Ignored undeclared reads require recheck; ordinary untracked source remains covered and changes invalidate it. |
| [decision-host-update-interruption.py](decision-host-update-interruption.py) | Actual kills immediately before/after host delta commit; exact recovery without effect redispatch or invented provider completion. |
| [decision-resolved-progress.py](decision-resolved-progress.py) | Direct, changed-managed and legacy completed plans retain declared status across stale freshness, migration and restart. |
| [decision-v5-upgrade.py](decision-v5-upgrade.py) | An actual prior writer creates v5 state; current v6 migration preserves progress, rejects old byte cursors/readers and retains history rollback. |
| [decision-continuation-live.py](decision-continuation-live.py) | Real same-lineage provider continuation across fresh processes, current input revisions and a same-size/restored-mtime source amendment. |
| [decision-live-compare.py](decision-live-compare.py) | Actual-provider linked-file tasks with independent exact values, chain-order receipts, unchanged files, full request bindings and reconciled usage. |
| [decision-review-live.py](decision-review-live.py) | Actual review-role source read after writer-only C6 scratch; inspects the published review view and final request bindings. |
| [candidate-observation-consumer.rs](candidate-observation-consumer.rs) | Ordinary consumer of production full managed Git observations, with original and shared capture fences. |
| [candidate-observation-replay.py](candidate-observation-replay.py) | Exact identity correspondence, actual concurrent atomic writer, source/index preservation, rejection and warm paired full-capture costs. |
| [candidate-observation-live.py](candidate-observation-live.py) | Actual managed design/write/gates/review under both capture policies; exact CSV candidate, qualified acceptance and unchanged HEAD/index. |
| [candidate-observation-interruption.py](candidate-observation-interruption.py) | Actual capture-boundary changes/kills, invalid Git metadata and fresh exact reacquisition without inference. |
| [candidate-observation-resume.py](candidate-observation-resume.py) | Real managed writer interruption and durable fresh-process continuation under source/HEAD amendments and capture-policy rollback. |
| [README.md](README.md) | Build, replay, scope, measurement and storage guidance. |
| [link.py](link.py) | Links the consumer against Cargo's actual freshly built dependency cohort. |
| [reuse-consumer.rs](reuse-consumer.rs) | Line-oriented public workspace-tool/context consumer; includes shared setup from the checkpoint-cache consumer. |
| [replay.py](replay.py) | Seeded pages, source/access changes, current policy, pending state and invocation lifecycle checks. |
| [pressure.py](pressure.py) | Exceeds decode retention, revisits sources, and rejects repeated loads of omitted/obsolete history. |
| [observed-results.json](observed-results.json) | Small final measured results; raw source/trace corpora stay outside the repository. |
| [compare.py](compare.py) | Exact pre/post API-output comparison with Linux logical IO measurements. |

Production owners: `peritus-product-runner` inspection and local-context assembly; the composable
estimator belongs to `peritus-agent`. See [parent waypoint](../AGENTS.md).
