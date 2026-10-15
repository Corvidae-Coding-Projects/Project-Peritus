# crates/app/peritus-daemon/src/product_control/inputs — navigation

## Responsibility

Implementation pieces for `inputs.rs`: Host-only request binding; clients cannot manufacture an incorporation grant.

## Files in this folder

| File | Role |
| --- | --- |
| [archive.rs](archive.rs) | Private immutable request artifacts installed with the control receipt. |
| [files.rs](files.rs) | Exact file context from already authenticated immutable artifacts, never ambient path reads. |
| [manifest.rs](manifest.rs) | Sealed, content-free provenance. Raw model messages never enter public projections. |
| [observation.rs](observation.rs) | Provenance for host-validated image observations, separate from governing input. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [archive/](archive/AGENTS.md) | Implementation pieces for `archive.rs`: Private immutable request artifacts installed with the control receipt. |

Owning module: [inputs.rs](../inputs.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../AGENTS.md) · [Repository map](../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
