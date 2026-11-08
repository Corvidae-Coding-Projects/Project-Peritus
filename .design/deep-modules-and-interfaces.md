# Deep modules and simpler interfaces

Scope: Peritus across its active Cargo workspace, starting at `fb38b4c18` on develop.
The objective is to move complete responsibilities behind smaller interfaces, preserve all
features, and reduce the number of invariants that callers must coordinate. File consolidation
and wrapper count alone do not establish module depth. Existing authority, exact archives,
wire formats, model choices, and durable recovery identities must remain intact.

## Confirmed coordination leaks

1. Provider context: callers invoke metadata preparation, request-profile resolution, a separate
   context-target getter, and output fitting in order. Output fitting can reread mutable metadata
   after profile construction. A prepared context should own one immutable numeric snapshot;
   provider transport checks remain a separate physical constraint.
2. Workflow stages: the execution bridge passes the definition, stage, role, and job separately
   and supplies a caller-built list of allowed predecessor outputs. The job already owns the
   manifest and checkpoint, so it should resolve the pending stage and its declared inputs.
3. Agent settings: commands, key handlers, renderers, and connection recovery coordinate menu
   stage, role, staged model, and destination fields. The selection module should own legal
   transitions and expose a read-only view plus explicit query/save actions.

## Delivery

First replace the multi-step provider contract with a prepared-context value. Update Codex,
ChatGPT, OpenCode, hosted adapters, effort decorators, the developer loop, reviewer/writer
projection, and native examples together. Freeze shared-window and output constraints in that
value rather than consulting the catalog again while building the same request.

Then give workflow execution a manifest-bound stage view. Keep host permission checks and
publication ordering in their current owners; a module package never gains host authority.
Finally consolidate agent-selection transitions behind their owning state object while keeping
the guided menus, independent effort control, exact commands, and daemon acknowledgements.

Use existing real-interface regressions and add only cases that distinguish the changed
boundaries. Compile all active consumers, replay relevant native interfaces, and record both
the inspected scope and verification limits. An API survey is not a whole-source correctness
review. Unchanged domains should not receive speculative abstractions merely for uniformity.

## Alternatives and compatibility

Renaming helpers or collecting arguments into a bag would leave coordination with callers.
One universal application manager would combine authority, persistence, transport, and UI
ownership. The preferred changes give each existing owner a complete operation and preserve
those boundaries. Internal Rust callers migrate together; no durable or wire-format migration
is intended. The active installed daemon remains separate from source and is not interrupted
by these refactors.

## Implementation and coverage record

The current Cargo metadata identifies 81 active workspace packages. The workspace inventory
and compiler checks cover those package interfaces; they do not imply that every source line
or every runtime path was reviewed. The implementation follows the three confirmed leaks
through their callers rather than applying uniform wrappers to unrelated owners.

| Inspected boundary | Ownership change | Migrated consumers |
| --- | --- | --- |
| Provider core/catalog and numeric discovery | `prepare_context(output, cancellation)` returns `PreparedModelContext`; profile resolution and reply fitting share one snapshot. The free profile resolver becomes crate-private. | Codex, ChatGPT, OpenCode, hosted HTTP composition, effort decorator, developer loop/request construction, writer/reviewer projection, live consumers. Other adapters retain their configured-profile default. |
| Declarative jobs, checkpoint publication, stage execution and read-only tools | `Job::pending_stage` resolves one `PendingStage`; declared reads and consumed completion are bound to that stage and package. | Product-runner stage request, local-memory namespace, role selection, progress, tool executor and publication. `ModuleTools::new` takes two arguments instead of five. |
| Terminal model commands, discovery responses, key routing, rendering and disconnect recovery | A private `Page` enum makes effort-without-model impossible. `ModelSetup` owns navigation/cursors; `ModelDestination` owns shared scope fences; saving names the role explicitly. | Guided `/model`/`/models`, direct shortcuts, standalone `/effort`, model/effort rendering and existing response/timeout/disconnect paths. |

The module job schema, artifact digests, application wire protocol, configured model identity,
host tool permissions, stage publication ordering and provider transport validation retain
their existing owners and formats. A stage port grants predecessor visibility, not host
authority. Native executable plugins use their separate ABI and are unchanged.

Foundation, state, runtime, tool implementations, analysis, observability, external plugins,
packaging, browser UI and the separate journal-backed orchestration drivers received no
internal redesign in this pass. Their Rust consumers are checked by the workspace compiler;
their internals and platform behavior are outside this source-review/behavioral coverage.
This is a cross-cutting implementation, not an exhaustive whole-codebase correctness audit.

Rollback is an atomic source revert of the changed Rust interfaces and their callers. There
is no on-disk migration to undo. Older out-of-tree Rust callers must migrate from the removed
`request_profile`, `context_input_target` and provider-level output fitting methods to the
prepared context; retaining those coordination APIs would undermine the new boundary.

Delivery verdict: **ready** for these three boundaries. The
[verification record](../docs/testing/deep-modules-2026-11-07.md) contains actual commands,
native/public-consumer observations, preserved regression cases and coverage limits.
