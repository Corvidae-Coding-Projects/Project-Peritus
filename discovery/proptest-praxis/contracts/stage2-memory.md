# Stage 2 memory replay, retrieval and lifecycle pilot

Status: source implementation ready; serial build, search, and mutation calibration pending. Baseline `d106c9b13318fbcdd3d816ed63ae48532806fa7e`. No production edits or retired test package dependency.

## Actual source and caller grounding

Read package manifests/navigation and all public memory source owners: `claim`, `confidence`, `error`, `evidence`, `feedback`, `identity`, `lifecycle`, `scope`, `tombstone`, `record` and `record/transitions`, `index/types`, `index/rebuild`, `index/canonical`, and `retrieval/{types,filter,ranking,plan,output}`. The production `verified` helpers are not used by this oracle.

`crates/orchestration/peritus-agent/src/runtime/context.rs::prepare_context` is an actual caller of public `retrieve`; it requires the memory query's role to match the agent role, then converts selected inert memories into quoted context evidence. This pilot exercises the public library boundary only. The interactive ProductRunner's local-context `MemoryRecord` is a different type and storage owner. No daemon state, artifact archive, provider request assembly, or AgentDriver context materialization is qualified here.

## Independent models

An ordinary map of logical identities stores case-defined material, scope dimensions, revision, explicit lifecycle state, created/reviewed/state observations, features, confidence, evidence counts, and feedback. A separate list stores deletion revision bounds. Production record snapshots remain retained as replay inputs, but never choose future operands or expected state. Every mutation checks the returned record fields, rebuilt active map, three secondary posting families, and latest-only versus full-history canonical index equality.

The retrieval oracle uses an explicit ordered exclusion truth table, a subset comparison over case-defined scope dimensions, a literal role exclusion table, integer arithmetic for the six public ranking components, stable ID tie breaking, and an independently sorted greedy whole-record budget admission model. It invokes no production filter, ranking, scope-compatibility, feature-lookup, certificate, or `verified` helper. Hash and public checked constructors construct legal real inputs; SHA-256 correctness and identifier ordering implementations are not independent targets here.

Direct retrieval accepts a unique latest record per ID in arbitrary order; the index builder instead requires full replay ordered by `(ID, revision)`. The harness preserves this distinction. It compares direct retrieval against the full latest map, indexed retrieval against the active/unforgotten subset, and their selected outputs. Their explanation sets intentionally differ because the index omits inactive/deleted entries.

Deletion suppresses revisions at or below its bound. The public index contract allows later imported active revisions above a tombstone; this pilot does not invent eternal deletion or authorization semantics. `Publish` imports a checked later initial-active snapshot; `Review`, quarantine/release, expiry, supersession, and forgetting use the actual lifecycle methods with separately modeled legal/error states.

## Generated histories and observations

- One to eight stable identities and zero through `min(max_steps,128)` operations; no rejection filtering, no hidden random choices, no silently skipped absent operands. Selectors map through the reference identity map, so shrinking retains executable operations. An explicit empty fixture covers empty index and retrieval.
- Repeated imported updates, equal-byte-length different-content replacement, review, quarantine, release, expiry, supersession, deletion, rebuild, exact query, and legal reversed record/tombstone retrieval input.
- Fresh, repeated-revision, and repeated-observation transition attempts. Terminal states have their documented error precedence. Failed attempts are followed by real index reconstruction and continued operations.
- Exact and broader scope matching across project/workspace/repository/actor/role fields; writer/fixer/evaluator/evolver visibility and reviewer/gate-runner/daemon exclusions.
- Shared feature keys with differing digests and query weights, required keys, all seven claim categories, confidence, unsupported evidence, review age, later epochs, future observations, expiry, negative feedback and contradiction thresholds.
- Single-component and mixed ranking, stable-ID ties, result count versus byte-independent caller token estimates, and first/all eligible cost minus one/exact/plus one budgets. Zero query allocation must be rejected by the real query constructor.
- Explicit unordered replay and duplicate identity rejection, separately from legal input reordering to the retrieval API.

Counters distinguish actual index rebuilds, active records checked, postings checked, retrievals, selected/explained/excluded candidates by reason, exact/broader and evidence/excluded-role queries, budget partitions, operation families, successful lifecycle changes, rejections, deletions, imported revisions, equal-size different-content updates, and legal versus invalid input reordering. Root must inspect actual metric reachability; case volume alone is not coverage.

## Mutation calibration

`calibration/stage2-memory.patch` deliberately selects the first rather than latest snapshot for each lineage in `MemoryIndex::rebuild`. `calibration_case` publishes one active record, quarantines it in a later revision, and rebuilds. The independent latest-state map requires no active posting; the mutant leaks the old active snapshot and must be caught. Generated searches should find the same mechanism through quarantine, review/content replacement, expiry, or deletion histories. Apply only in the dedicated isolated calibration checkout and retain compiled baseline, mutant behavioral failure, exact/minimized witnesses, and restored passes. No production mutation has been applied by this worker.

## Explicit limits

This is a bounded public-library correctness pilot. It does not exercise durability, unauthorized memory updates, artifact deletion/privacy, evidence authenticity, cross-process state, index serialization hashing independently, maximum production allocations, larger feature sets, every possible role/scope combination, all constructor rejection classes, overlap/digest-conflicting tombstone input, or real model behavior. All generated observations use epoch 1; queries additionally use epoch 2. Material replacements import later legal snapshots rather than claiming a missing mutable material-update API. Scope dimensions are input-bound, never ambient filesystem paths. No bugs or campaign pass are claimed until actual build/search/calibration evidence exists.
