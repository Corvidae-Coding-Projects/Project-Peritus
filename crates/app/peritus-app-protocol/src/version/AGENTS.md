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

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
