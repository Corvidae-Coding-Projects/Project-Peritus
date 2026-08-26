# E1 harness materialization

`peritus-harness` is the durable production boundary between reviewed harness source and the exact
files used by an agent workspace. It loads one committed manifest through C1, validates a complete
typed component graph, constructs immutable content-addressed revisions, and materializes a
selected revision through one authorized C1 patch and candidate operation.

E1 does not evaluate, diagnose, promote, or activate a production harness. It never grants a B1
capability and never writes repository or Git state directly. E2, E3, and F0 consume E1's immutable
revision and receipt facts for those later decisions.

## Repository layout

The only harness entry point is:

```text
.peritus-harness/
  manifest.toml
  components/
    ... every declared source file ...
```

The schema-v1 manifest is strict TOML. Unknown fields, non-UTF-8 manifest bytes, an unsupported
schema, a duplicate component ID, a duplicate source or target path, or a declaration beyond the
configured limits rejects the complete load. Component payloads are opaque bytes and may be
binary.

Production loading uses `ReadOnlyWorkspace` metadata, listing, and bounded no-follow reads. The
recursive regular-file inventory below `.peritus-harness/components/` must exactly equal the
manifest declarations. Missing, undeclared, symlinked, special, oversized, wrong-size, or
wrong-digest files do not produce a partial harness.

## Component model

Every declaration records:

- a stable component ID and closed component kind;
- schema version, supported compatibility interval, and provider/platform feature requirements;
- normalized source and materialization target paths;
- exact byte length, SHA-256 source digest, media type, and optional executable-artifact digest;
- owner and provenance;
- declared descriptive authority; and
- dependency IDs with required kind, schema interval, and optional exact digest.

The closed catalog covers base/system instructions, roles and prompts, tool descriptors/schemas/
implementations/exposure, middleware and context transforms, skills and references, sub-agents and
collaboration, memory schema/selection/ranking/retention/injection, gates and parsers,
orchestration/termination, provider capability/profiles, observability/redaction/analysis, and
evolution strategies/metrics.

Security-root policy, human authority, sealed evaluators, trust-boundary definitions, and
production-promotion rules are protected controlled assets. Protection is derived from compiled
component-kind policy; manifest text cannot lower it.

## Checked graph

`CheckedHarnessGraph` exists only after complete validation. The checker resolves every dependency
and rejects missing or self edges, duplicates, cycles, incompatible kind/schema/digest
requirements, unsatisfied feature requirements, and invalid protected dependencies. Its
topological order is deterministic, using stable component ID as the tie breaker.

Authority declarations describe the maximum exposure that later runtime composition may request;
they do not grant authority. Each kind has a compiled authority ceiling, and transitive dependency
authority must also fit the depender's ceiling. Actual effects remain subject to B1 capabilities
and the C1/C4/C6 target-owned authorization boundaries.

## Revision identity and history

A genesis revision is constructed only from a checked graph and the exact verified component
contents. Its domain-separated digest commits the manifest and graph digests, complete canonical
declarations, content sizes/digests, executable digests, and lineage seed. `HarnessId` is derived
from that genesis identity and remains stable for the lineage.

Every successor names the exact predecessor digest, advances the logical revision number once,
and recomputes its full content identity. Branches are permitted, so the full SHA-256 digest—not a
display revision number—is authoritative. A successor cannot add, remove, rename, reorder, or
change any field, bytes, dependency, authority, or executable binding of a protected asset.

`HarnessHistory` is an append-only bounded DAG. It rejects another genesis, an orphan successor,
identity conflict, and predecessor/number disagreement. Ancestry queries support rollback without
rewriting or deleting descendants.

## Materialization planning

A materialization plan binds all of these facts before an effect is requested:

- harness ID, full revision and graph digests;
- target workspace ID, generation, logical revision, snapshot, and tree;
- forward or ancestor-rollback reason;
- prior E1 receipt when one exists;
- canonical create/replace/delete operations and expected preimages;
- exact output artifact digest/size per target path; and
- command/event/outbox identities plus compiled size/count limits.

Creates and replacements cover the target graph. Deletes are permitted only for paths proven to
be owned by the exact prior E1 materialization receipt and absent from the target graph. Unrelated
workspace files are never part of an E1 delete plan. `.git`, `.peritus`, and `.peritus-harness`
remain protected targets.

The compiled component-count, per-file, and aggregate materialization ceilings are the exact C1
atomic `PatchSet` limits. A manifest may tighten them but cannot describe a revision that the sole
workspace mutation boundary could never apply.

Before constructing the C1 `PatchSet`, the executor calls `ArtifactStore::read` for each payload.
That API requires active finalized metadata, enforces the requested byte ceiling, and verifies the
exact returned size and digest. `materialization_authorization_payloads` deterministically exposes
the inert patch payload and the candidate payload predicted from that same patch identity, so the
two independent authorizations can be obtained without duplicating patch logic. E1 then calls
`WorkspaceGateway::apply_patch` and
`WorkspaceGateway::create_candidate` under separate exact authorizations. A successful receipt
records the patch, action, before/after workspace, Git commit/tree, C1 manifest artifact, and
complete output inventory.

## Durable protocol

The E1 aggregate uses:

| Purpose | Stable identity |
|---|---|
| harness command frame | B3 family 79, schema 1 |
| harness event frame | B3 family 80, schema 1 |
| complete harness checkpoint | B3 family 81, schema 1 |
| C0 aggregate kind | `Harness`, tag 13 |
| C0 checkpoint namespace | `0xE101` |

Decoded frames remain inert. Checked constructors and the reducer validate all domain state before
anything can be registered or materialized. Unknown tags, invalid lengths, noncanonical sets,
truncation, and trailing bytes reject.

Registration commits the immutable revision and its finalized artifact dependencies. Planning a
materialization atomically commits the event, complete checkpoint, artifact roots, command
idempotency record, and stable outbox directive before C1 is invoked. Success or failure can settle
only the exact pending plan and acknowledges only its exact claimed outbox fence in the same C0
event/checkpoint transaction.

## Restart and reconciliation

Recovery replays from genesis through the same reducer and compares the canonical result with the
complete family-81 checkpoint. Sequence, predecessor, event, prior-state, command, revision, graph,
receipt, and artifact identities must all agree.

For a pending materialization:

1. An untouched exact target permits redelivery of the same idempotent directive.
2. An exact matching patch/candidate already created by C1 permits recording the retained receipt.
3. A stale workspace head requires a newly checked plan; the old plan is not rebound.
4. A partial, dirty, indeterminate, or conflicting observation remains pending reconciliation or
   becomes a typed quarantined failure. It is never guessed into success.

The rebuildable projection exposes graph/revision summaries, ancestry, protected inventory,
pending work, delivery state, materialization receipts/failures, and artifact roots. It provides no
mutation or production-promotion method.

## Rollback

Harness rollback selects an immutable ancestor of the stated source revision and runs the normal
materialization pipeline with an explicit rollback reason. The result is a new C1 candidate and
E1 receipt. The revision history, descendants, and prior receipts remain intact. E1 does not move a
production pointer; a later F0 decision owns production activation and rollback policy.

## Schema migration

C0 schema version 6 widens only the constrained journal aggregate-kind columns from tags 1–12 to
1–13. It copies heads and events in canonical order, verifies row counts, rebuilds the command
index, and publishes schema/user version 6 in one migration transaction. The migration requires a
completed whole-file backup.

The frozen v5 fixture proves every historical tag and event byte survives the migration. The test
then appends tag-13 family-80 data, runs the journal integrity scan, restores the backup, and checks
the exact v5 rows and `user_version` again. After real tag-13 data exists, use the v5 backup or a
forward repair for binary rollback; an old v5 binary cannot open a v6 store.

## Verification

Focused verification covers domain, manifest/inventory, graph, immutable revision history,
materialization planning/execution, protocol fixtures, durability, replay/restart, migration,
artifact reads, projection, and the independent A2 harness conformance catalog. Strict Verus runs
with `--no-cheating`; ordinary APIs are audited separately.

Run heavyweight commands serially and set `CARGO_BUILD_JOBS=1`. The complete merge authority is
one local `just gate-a`, followed by the hosted Linux/macOS/Windows Gate A and Foundation matrices.

## Operational boundaries

- Do not hand-edit a registered revision or receipt. Commit a successor manifest/component set.
- Do not treat a loaded manifest, decoded frame, plan, projection, or receipt as B1 authority.
- Do not materialize around C1 with raw filesystem or Git commands.
- Do not delete a target unless an exact prior E1 receipt proves ownership.
- Do not treat materialization as evaluation or production promotion.
- Preserve the original command and plan identity while resolving an indeterminate commit.
