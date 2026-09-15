# Feature: GAP-04 context selection, rendering, and resume correspondence

## Summary

Close GAP-04 by proving the complete deterministic relationship from C6 context and working-state
inputs through selection, compaction, rendering, product-runner assembly, persistence, recovery,
and the next provider request. Preserve the already reviewed graph and reuse kernels. Work starts
after PR #77 lands, and remains tracked by Crosslink issue #80.

The current code already contains a substantial production integration under
`crates/app/peritus-product-runner/src/local_context`. The crate README statement that working
state is not wired into the product is stale. GAP-04 will qualify and strengthen this existing path
rather than introduce a second context system.

## User-visible behavior

- Identical admitted context, role, provider profile, and working state produce the same bounded
  provider messages.
- Required visible material and its dependency closure are either rendered intact or rejected with
  the correct typed error.
- Optional material is omitted atomically and deterministically when it cannot fit.
- Compaction cannot summarize protected instructions, forge lineage, or replace content without a
  strict size reduction.
- Context saved before a retry or restart is reconstructed without losing eligible evidence,
  changing authority, or reviving stale state.
- Newly observed files, inputs, and conversation revisions invalidate only the state whose explicit
  validity conditions changed.

## Requirements

1. `select_context` must specify its exact selected set, omission set and reasons, dependency
   closure, ranking, render order, and token/node/byte accounting on success, plus the first
   applicable rejection on failure.
2. `validate_compaction` and `replace_validated_compaction` must specify the exact accepted source
   ranges, protected-source rejection, replacement metadata, lineage, graph delta, and strict token
   reduction.
3. `build_render_plan` must map every selected node exactly once while preserving content, role,
   provenance, authority, trust, digest, type, accounting, and deterministic order.
4. Working-state ingestion, refresh, delta, protocol, event replay, wire encoding, and bounded
   rendering must specify their exact state transitions and unchanged-state rejection behavior.
5. Product-runner local-context storage, recovery, ingestion, assembly, and memory tools must retain
   the exact run/workspace/task/role/conversation binding enforced by C6.
6. `peritus-agent` and product-runner callers must consume the exact selected/rendered view. Message
   encoding may add protocol framing but may not reorder content or promote authority.
7. Provider retry and daemon/process restart must reconstruct an equivalent eligible prompt view.
   Differences are allowed only for fresh inputs explicitly represented in the state transition.
8. OBL-0138 through OBL-0140 remain in progress during implementation. A fresh source map must add
   any missing GAP-04 obligations without weakening or prematurely discharging existing records.

## Acceptance criteria

| Area | Required evidence |
|---|---|
| Selection | Exact contracts and tests for required closure, hidden dependencies, optional ranking, atomic omission, competing capacity limits, accounting, and order |
| Compaction | Tampered range/digest/lineage and protected-content rejection; exact successful replacement and strict reduction |
| Working state | Exact reducer/replay contracts, stale-binding and invalidation tests, wire round trips, corrupt-history rejection, and deterministic bounded rendering |
| Production assembly | Caller tests binding C6 inputs and output segments to the exact messages passed to the provider |
| Resume | Retry and restart tests comparing pre-restart and reconstructed eligible views, including unpublished-tail and stale-generation cases |
| Security | Poisoning tests show repository, tool, provider, memory, agent, and compacted text stays non-authoritative |
| Proof quality | Negative mutations for wrong selection, omission, ordering, compaction, state update, or replay fail the corresponding contract |
| Qualification | Strict no-cheating Verus, affected ordinary tests, strict Clippy, formatting, rustdoc, architecture, ordinary API, generated artifacts, durability callers, source-bound independent review, proof-impact reconciliation, and hosted CI all pass |

## Current architecture

The pure C6 path is centered in:

- `crates/orchestration/peritus-context/src/selection/plan.rs::select_context`;
- `selection/closure.rs` and `selection/ordering.rs`;
- `compaction/validation.rs::validate_compaction` and
  `compaction/replacement.rs::replace_validated_compaction`;
- `render.rs::build_render_plan`; and
- `working/{state,delta,protocol,event,selection,wire}.rs`.

The production composition path is centered in:

- `crates/orchestration/peritus-agent/src/runtime/context.rs::{prepare_context,render_messages}`;
- `crates/orchestration/peritus-agent/src/developer/context.rs::prepare_messages`;
- `crates/app/peritus-product-runner/src/local_context`, especially `port.rs`, `assembly.rs`,
  `memory/recovery.rs`, `memory/checkpoint.rs`, and `storage.rs`;
- `crates/app/peritus-product-runner/src/control/{context,compaction}.rs`; and
- `crates/app/peritus-daemon/src/product_control/{context,compaction}.rs` plus the product-run
  restart callers.

The graph/cycle and reusable-section kernels already have reviewed evidence under
`docs/formal-coverage-evidence/context-graph-reuse`. GAP-04 consumes those results and reopens them
only if a production correspondence test exposes a real defect.

## Proposed design

### 1. Freeze the source and obligation map

After PR #77 merges, rebase this branch onto the resulting `origin/develop`. Record the exact base,
all public and crate-private functions in the path above, their production callers, current tests,
existing specifications, and missing relationships. Separate already proved graph/reuse behavior
from the GAP-04 work.

### 2. Strengthen pure C6 contracts

Add executable specifications to the existing selection, compaction, rendering, and working-state
functions. Prefer small ghost models and helper lemmas that describe the production values. Do not
create verification-only implementations that callers bypass.

### 3. Prove composition at the caller seams

Expose only the minimal read-only specification views needed for product-runner and agent callers.
Relate the C6 plan to `PreparedView`, the persisted checkpoint manifest, recovered `LocalMemory`, and
the final `Message` sequence. Preserve current public APIs and wire formats unless a reproduced
correctness defect requires a compatible migration.

### 4. Close retry and restart correspondence

Use deterministic fixtures that commit observations and checkpoints, reopen storage, apply any
explicit refresh event, assemble the next view, and compare it with the uninterrupted execution.
Also test corruption, missing artifacts, stale generation, binding drift, and unpublished tails.

### 5. Freeze and qualify one final candidate

Run focused checks while editing. Once the source is final, regenerate proof impact, run every
manifest-derived affected package command once, retain the outputs, obtain independent review of
that exact source, and let hosted CI qualify the pushed head. Keep authorization bookkeeping
separate from the implementation candidate.

## Data and compatibility

Default to no schema or public API change. Existing context graph, working-state, checkpoint,
conversation, and generated protocol encodings must continue decoding their current fixtures. If a
reproduced gap requires new persisted data, introduce a versioned dual reader and test old-state
recovery before writing the new version.

## Failure handling

All mismatched bindings, invalid closures, arithmetic overflow, protected compaction sources,
tampered artifacts, corrupt histories, and stale generations fail before publishing a new view or
checkpoint. Rejections retain the prior durable state. No fallback may silently drop required
content, reset working memory, or call a cloud compactor.

## Security considerations

Only host policy and literal current user requirements may become instructions. Repository, tool,
provider, memory, agent, review, and compacted material remains quoted non-authoritative evidence.
Credentials remain excluded from derived working entries. Context selection cannot grant a tool
capability or count as repository grounding, tool execution, review, or acceptance evidence.

## Verification

Focused baseline and final package commands:

```sh
CARGO_BUILD_JOBS=2 cargo test --locked --package peritus-context --all-targets --all-features
CARGO_BUILD_JOBS=2 cargo verus verify --package peritus-context --all-features --locked \
  --check-toolchain --fwd-verus-args-to roots -- --no-cheating --rlimit 20
CARGO_BUILD_JOBS=2 cargo test --locked --package peritus-agent --all-targets --all-features
CARGO_BUILD_JOBS=2 cargo test --locked --package peritus-product-runner --all-targets --all-features
```

Add the exact daemon durability tests and all proof-impact-derived package gates after the source
map identifies affected callers. Final qualification also includes workspace formatting, strict
Clippy, rustdoc warnings, architecture, ordinary API, generated artifacts, source/evidence digest
checks, independent review, and current-head hosted workflows.

## Rollout and rollback

Land GAP-04 as its own PR after #77. Pure contract/test changes roll back with the implementation
commit. Any required persisted-schema change must document its compatibility floor and retain a
reader for the previous version; restoring an old binary over newly written incompatible state is
not an acceptable rollback plan.

## Open questions

None needed to begin. The source map may reveal a real compatibility choice; reproduce it and raise
that choice before changing wire or persistence behavior.

## Out of scope

- Cryptographic execution, clocks, persistence media guarantees, and external/provider effects:
  GAP-06.
- Broader kernel, agent, protocol, product lifecycle, release, and security composition: GAP-05.
- Protected CI bootstrap and final campaign delivery: GAP-07 and GAP-08.
- Reworking reviewed graph/reuse kernels without a reproduced defect.
