# peritus-context

## Folder guide

provenance-aware context planning for Peritus.

Active root-workspace package; declarations and targets are in [Cargo.toml](Cargo.toml).

Use [AGENTS.md](AGENTS.md) for file-level responsibilities, child modules, parent context, and nearby implementation owners.

### Local files

| File | Role |
| --- | --- |
| [Cargo.toml](Cargo.toml) | Cargo package/dependency/target declarations for `peritus-context`. |
| [README.md](README.md) | Human guide and operating/background notes for this folder; AGENTS.md provides the local navigation map. |

### Subfolders

| Folder | Role / next waypoint |
| --- | --- |
| [src/](src/AGENTS.md) | Provenance-aware context planning for Peritus. |

Production C6 provenance graph, selection, compaction, token-budget, and render-plan contracts.

This H-class crate uses the canonical `peritus-codec` SHA-256 boundary to bind caller-supplied
context bytes. Its deterministic graph, selection, accounting, compaction validation,
render planning, and run-knowledge selection logic performs no ambient I/O.

Graphs validate strictly increasing IDs before binary-search lookup. Dependency closure visits
reachable nodes once and returns canonical graph order, preserving required-first admission,
atomic optional closures, hidden-dependency diagnostics and exact accounting without repeated
whole-graph passes. Coding-run hosts execute synchronous planning away from their IPC executor.

Role delta packets are bound back to exact context-node digests before rendering. The resulting
`ReusableContextSelection` retains the full run-knowledge provenance and distinguishes changed
authoritative facts, current references, and navigation-only summaries.

The `working` module adds bounded task-local investigation state: provider-independent scope,
exact artifact-range observation handles, source-backed atomic updates, canonical dependency
validation, and transitive stale/superseded status. Every working entry is non-authoritative.
Hosts supply verified, redacted inputs; these reducers perform no storage or provider calls.
The product runner integrates this engine through its local-context storage, checkpoint, recovery,
assembly, and memory-tool adapters; those effectful adapters remain outside this pure crate.

Host-owned `WorkingEvent::HostDelta` conjoins a proposal's dependencies with the originating
request's host constraints. Unknown/changed inputs retain structurally valid entries as stale;
ordinary model deltas keep their stricter rejection rules. `HostInvalidate` makes legacy
unproven conclusions stale without deleting progress or exact sources. Model updates cannot
set host bindings, stale/superseded status or revive a host-bound entry through a plain delta.

Working codec versions 1–3 remain readable. Ordinary events/snapshots/pages retain version-2
encoding. Version 3 adds event tags 4/5 for host delta/invalidation. Version 4 bound entries retain
the source prefix/request fingerprint and original declared investigation status, separately
from effective freshness. Older bound-entry status is inferred only when known.
A snapshot uses version 4 only if bound entries
exist; an entry page uses it only if that page contains a bound entry. Observation, header and
protocol pages keep their existing bytes. Unknown versions/tags, invalid bindings and
noncanonical mixed frames reject. The product host commits a version-6 checkpoint reader floor
before writing a new host event, and preserves that floor when switching back to history views.
Older product readers refuse that lineage before replaying its tail. See the
[I4 qualification report](../../../docs/testing/execution-efficiency-i4-decision-views.md).

## Behavioral work

Use the [project real-testing policy](../../../README.md#real-testing) and the actual interfaces owned by this crate. The [local map](AGENTS.md) identifies entrypoints and implementation owners.
