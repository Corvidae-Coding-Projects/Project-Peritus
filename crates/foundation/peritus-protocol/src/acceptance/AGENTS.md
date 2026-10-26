# crates/foundation/peritus-protocol/src/acceptance — navigation

## Responsibility

Version-one acceptance-contract protocol families.

## Files in this folder

| File | Role |
| --- | --- |
| [contract.rs](contract.rs) | Digest-bound complete canonical acceptance contracts. |
| [dto.rs](dto.rs) | Stable data-only projections of non-copy B2 contract components. |
| [mod.rs](mod.rs) | Version-one acceptance-contract protocol families. |
| [values.rs](values.rs) | Canonical B2 contract component encodings. |

## Working in this area

Start at [`mod.rs`](mod.rs) for the module surface, then follow its declarations into the focused files and child waypoints above. Trace at least one caller and the relevant error or recovery path before moving responsibilities; the parent package map identifies the public boundary and adjacent owners.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
