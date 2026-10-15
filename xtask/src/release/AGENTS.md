# xtask/src/release — navigation

## Responsibility

Implementation pieces for `release.rs`: Tag-bound GitHub release staging behind direct reviewed Cargo commands.

## Files in this folder

| File | Role |
| --- | --- |
| [publication.rs](publication.rs) | Complete target inventory and tag-bound bootstrap staging, without publication authority. |
| [rebuild.rs](rebuild.rs) | Reviewed, failure-propagating entry points for native rebuild observations. |

Owning module: [release.rs](../release.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
