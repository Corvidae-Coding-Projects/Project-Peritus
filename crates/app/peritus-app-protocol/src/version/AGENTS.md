# crates/app/peritus-app-protocol/src/version — navigation

## Responsibility

Protocol version, feature, limit, and compatibility negotiation.

## Files in this folder

| File | Role |
| --- | --- |
| [feature.rs](feature.rs) | Canonically ordered protocol feature names. |
| [mod.rs](mod.rs) | Protocol version, feature, limit, and compatibility negotiation. |
| [negotiation.rs](negotiation.rs) | Deterministic, insertion-order-independent protocol negotiation. |
| [range.rs](range.rs) | Checked application-protocol versions and inclusive ranges. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [negotiation/](negotiation/AGENTS.md) | Implementation pieces for `negotiation.rs`: Deterministic, insertion-order-independent protocol negotiation. |

## Working in this area

Start at [`mod.rs`](mod.rs) for the module surface, then follow its declarations into the focused files and child waypoints above. Trace at least one caller and the relevant error or recovery path before moving responsibilities; the parent package map identifies the public boundary and adjacent owners.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
