# crates/orchestration/peritus-harness/src/domain — navigation

## Responsibility

Pure checked harness graph, immutable revision, and append-only history domain.

## Files in this folder

| File | Role |
| --- | --- |
| [authority.rs](authority.rs) | Closed descriptive authority vocabulary and non-minting sets. |
| [canonical.rs](canonical.rs) | Length-prefixed, domain-separated canonical encoding helpers. |
| [catalog.rs](catalog.rs) | Closed component and protection catalogs with compiled policy. |
| [compatibility.rs](compatibility.rs) | Schema compatibility and inert provider/platform feature requirements. |
| [component.rs](component.rs) | Closed component catalog and strictly validated declarations. |
| [component_canonical.rs](component_canonical.rs) | Canonical declaration encoding and checked reconstruction. |
| [content.rs](content.rs) | Exact opaque component-byte verification used before revision construction. |
| [error.rs](error.rs) | Stable failures emitted by checked harness-domain constructors. |
| [graph.rs](graph.rs) | Deterministic complete harness graph validation. |
| [graph_validation.rs](graph_validation.rs) | Focused graph algorithms and graph-canonical helpers. |
| [history.rs](history.rs) | Bounded append-only branched revision history and rollback ancestry. |
| [identity.rs](identity.rs) | Strong harness-domain identities and digest roles. |
| [limits.rs](limits.rs) | Compiled harness ceilings and manifest-controlled tightening. |
| [mod.rs](mod.rs) | Pure checked harness graph, immutable revision, and append-only history domain. |
| [revision.rs](revision.rs) | Immutable content-addressed harness revisions. |
| [value.rs](value.rs) | Validated textual and path values used by component declarations. |
| [verified.rs](verified.rs) | Executable invariant/refinement predicates for the local domain. Entry names: `component_ids_are_unique`, `dependencies_are_resolved`, `topological_order_is_complete`, `authority_is_non_widening`, `protected_assets_are_invariant`. |

## Working in this area

Start at [`mod.rs`](mod.rs) for the module surface, then follow its declarations into the focused files and child waypoints above. Trace at least one caller and the relevant error or recovery path before moving responsibilities; the parent package map identifies the public boundary and adjacent owners.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
