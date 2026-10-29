# crates/state/peritus-evidence/src/bundle — navigation

## Responsibility

Deterministic portable bundle planning, streaming assembly, and offline verification.

## Files in this folder

| File | Role |
| --- | --- |
| [assemble.rs](assemble.rs) | Preflighted, resumable, and atomic portable bundle assembly. |
| [format.rs](format.rs) | Fixed portable bundle framing helpers. |
| [mod.rs](mod.rs) | Deterministic portable bundle planning, streaming assembly, and offline verification. |
| [plan.rs](plan.rs) | Pure canonical portable bundle planning. |
| [preparation.rs](preparation.rs) | Incremental private staging with source hashes and exact retained write progress. |
| [verify.rs](verify.rs) | Framed incremental verification with an owned input, hashes, and exact continuation cursor. |

## Working in this area

Start at [`mod.rs`](mod.rs) for the module surface, then follow its declarations into the focused files and child waypoints above. Trace at least one caller and the relevant error or recovery path before moving responsibilities; the parent package map identifies the public boundary and adjacent owners.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
