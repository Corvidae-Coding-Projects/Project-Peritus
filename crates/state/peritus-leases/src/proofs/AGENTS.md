# crates/state/peritus-leases/src/proofs — navigation

## Responsibility

Proof decomposition for logical exclusivity, fencing, exact authority, and refinement.

## Files in this folder

| File | Role |
| --- | --- |
| [authority.rs](authority.rs) | Exact policy-use and lease-use intersection lemmas. |
| [exclusivity.rs](exclusivity.rs) | Logical exclusive-holder lemmas for `INV-006`. |
| [fencing.rs](fencing.rs) | Generation-fencing and stale-claim rejection lemmas. |
| [mod.rs](mod.rs) | Proof decomposition for logical exclusivity, fencing, exact authority, and refinement. |
| [refinement.rs](refinement.rs) | Refinement lemmas for executable versions and authority-time floors. |

## Working in this area

Start at [`mod.rs`](mod.rs) for the module surface, then follow its declarations into the focused files and child waypoints above. Trace at least one caller and the relevant error or recovery path before moving responsibilities; the parent package map identifies the public boundary and adjacent owners.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
