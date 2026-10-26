# crates/foundation/peritus-protocol/src/policy — navigation

## Responsibility

Version-one action and policy protocol families.

## Files in this folder

| File | Role |
| --- | --- |
| [action.rs](action.rs) | Complete canonical bytes for one proposed action intent. |
| [amendment.rs](amendment.rs) | Digest-bound canonical single-tier policy amendments. |
| [definition.rs](definition.rs) | Complete canonical checked policy definitions. |
| [dto.rs](dto.rs) | Stable data-only policy model used by the wire contract. |
| [mod.rs](mod.rs) | Version-one action and policy protocol families. |
| [rule_codec.rs](rule_codec.rs) | Canonical approval constraints, restriction rules, and layers. |
| [selector_codec.rs](selector_codec.rs) | Canonical policy selectors, permissions, validity, and use bounds. |
| [tags.rs](tags.rs) | Closed B1 policy enum tags. |

## Working in this area

Start at [`mod.rs`](mod.rs) for the module surface, then follow its declarations into the focused files and child waypoints above. Trace at least one caller and the relevant error or recovery path before moving responsibilities; the parent package map identifies the public boundary and adjacent owners.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
