# crates/foundation/peritus-protocol/src — navigation

## Responsibility

Stable canonical domain protocol for Peritus.

## Files in this folder

| File | Role |
| --- | --- |
| [lib.rs](lib.rs) | Stable canonical domain protocol for Peritus. |
| [primitive.rs](primitive.rs) | Canonical encodings shared by protocol message families. |
| [version.rs](version.rs) | Protocol version constants. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [acceptance/](acceptance/AGENTS.md) | Version-one acceptance-contract protocol families. |
| [agent/](agent/AGENTS.md) | Stable inert records used to persist and replay one D0 agent turn. |
| [bin/](bin/AGENTS.md) | Executable entrypoints for `peritus-protocol`; follow each binary into its package implementation. |
| [budget/](budget/AGENTS.md) | Version-one budget protocol families. |
| [lifecycle/](lifecycle/AGENTS.md) | Version-one lifecycle protocol families. |
| [policy/](policy/AGENTS.md) | Version-one action and policy protocol families. |
| [schema/](schema/AGENTS.md) | Reproducible schema, client declaration, and compatibility-corpus generation. |

## Working in this area

Start at [`lib.rs`](lib.rs) for the module surface, then follow its declarations into the focused files and child waypoints above. Trace at least one caller and the relevant error or recovery path before moving responsibilities; the parent package map identifies the public boundary and adjacent owners.

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
