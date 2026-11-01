# Stage 4 harness graph, history, and real materialization

Final bounded qualification is recorded in `../STAGE4-REPORT.md` and `../inventory/stage4-targets.json`; earlier worker-status notes describe their historical snapshot.

This public consumer exercises `peritus-harness`, actual file-backed C0/artifact storage, a disposable Git repository and registered detached C1 workspace, and `HarnessRuntime`. It uses the existing public `peritus-product-runner::MaterializationAuthorityContext` to commit actual kernel/capability/lease/epoch authority for the committed E1 plan. No effect permits or successful receipts are fabricated, and no production API visibility is changed.

## Source and caller grounding

Read harness package/source/materialization/runtime waypoints and active manifest; domain graph and graph_validation, component/content/catalog/value/identity/compatibility/authority/limits, immutable revision and history, materialization planner/snapshot/plan model/executor/receipt, aggregate command/state/settlement, runtime driver/types/ports, and durable commit/recovery. Trace current daemon harness dispatch/startup replay, product-runner public API and `developer_tools/command_runtime/materialization_authority.rs`, C1 gateway/open/writable/state/identity, structured Git repository/worktree/baseline, and shared artifact-store configuration/write APIs. The retained `docs/testing/reproducers/rsi-production/evolution/e1.rs` supplied a bootstrap example; each used constructor and authority seam was checked against current implementation. Its retained results are not treated as current evidence.

## Independent models and histories

Materialization cases carry one to three revisions, each containing a nonempty subset of four fixed portable paths with generated binary/UTF-8/CRLF content. The independent state is a `BTreeMap<path, exact bytes>` with regular-file mode. Expected operations are the ordered union of previously owned and newly declared paths: every new path installs its exact digest/length/mode and old preimage, and every removed old path deletes its exact prior content. The complete installed-byte total is independently summed. This is compared to the public plan before execution; post-execution filesystem bytes, deletion absence, non-executable mode on Unix, unrelated baseline bytes, and exact receipt file inventory are checked independently.

Each valid revision goes through real component artifact finalization, C0 genesis/successor registration, plan/checkpoint/outbox commit, a genuine outbox claim, real C1 patch and candidate creation, and atomic durable success/failure settlement. Optional reopen occurs after plan commit and before any C1 work: the checked public replay reconstructs `CommittedPlan`, pending/receipt counts must match, and the filesystem must remain unchanged. Completed state is reopened and compared exactly after each round. A final identical plan must return the retained receipt and perform no new materialization.

Fault histories create an unowned target or edit a previously owned target before planning; both must reject and preserve actual external bytes and durable registered state. Distinct real patch/candidate authorizations are deliberately swapped to test C1's exact payload/action gates. Wrong patch authority must preserve prior files. Wrong candidate authority occurs after the real patch and must retain a durable failed outcome with no success receipt; the independent filesystem model explicitly records those partial file effects rather than claiming rollback.

Graph cases reject duplicate declarations, missing dependencies, cycles, ancestor-path collisions, and wrong verified content. Branched history cases compare every ancestor pair and selected rollback to an independent parent-index list, preserve the exact snapshot after duplicate/orphan/foreign-lineage rejections, check branch-tip completeness, and decode/reopen the public canonical history snapshot.

## Metrics and calibration

Metrics record successful materializations, rejected effects, actual checkpoint reopens, deletions, durable failures with partial C1 effects, retained idempotent receipts, graph rejections, retained successors, ancestry comparisons, and history rejections. They count interactions, not exhaustive code coverage.

`calibration/stage4-harness.patch` inverts the prior-receipt path-membership check in the deletion planner. The explicit witness first materializes path A, then changes the revision to path B. The independent plan/path map must detect omission of A's deletion; generated multi-round masks reach the same interaction. This mutation remains unapplied until the isolated baseline/mutant/restored cycle.

## Boundaries and evidence status

The campaign uses actual C1 and real authority construction but starts no arbitrary model commands, external network traffic, or production repository changes. It does not qualify power loss, concurrent settlement races, artifact GC during execution, executable components, protected-asset amendment rules, complete component-kind policy matrices, or rollback materialization effects. Exact snapshot/receipt recovery is checked through the real C0 interface; a canonical round trip alone is not called durable execution.

Only formatting, whitespace, and mutation applicability are currently checked by this worker. Build/search/replay/calibration claims require final integrated-run artifacts from the root runner. Disposable workspace creation and all mutations remain under the campaign-owned temporary root.
