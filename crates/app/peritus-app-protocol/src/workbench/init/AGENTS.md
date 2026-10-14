# crates/app/peritus-app-protocol/src/workbench/init — navigation

## Responsibility

Implementation pieces for `init.rs`: Bounded, approval-first project initialization contracts.

## Files in this folder

| File | Role |
| --- | --- |
| [artifacts.rs](artifacts.rs) | Immutable initialization content references and bounded review pages. |
| [proposal.rs](proposal.rs) | Complete validated initialization proposals assembled from bounded observations. |
| [render.rs](render.rs) | Deterministic managed-section content and whole-file review rendering. |
| [validation.rs](validation.rs) | Structural validation for inert initialization paths and command tokens. |

Owning module: [init.rs](../init.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../AGENTS.md) · [Repository map](../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
