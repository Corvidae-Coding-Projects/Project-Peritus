# crates/analysis/peritus-debugger/src/report — navigation

## Responsibility

Checked report claims, complete validation, and canonical bytes.

## Files in this folder

| File | Role |
| --- | --- |
| [canonical.rs](canonical.rs) | Canonical schema-v1 report encoding. |
| [claim.rs](claim.rs) | Claim kinds whose constructors encode their evidence obligations. |
| [mod.rs](mod.rs) | Checked report claims, complete validation, and canonical bytes. |
| [validation.rs](validation.rs) | Complete report validation and validated wrapper. |

## Working in this area

Start at [`mod.rs`](mod.rs) for the module surface, then follow its declarations into the focused files and child waypoints above. Trace at least one caller and the relevant error or recovery path before moving responsibilities; the parent package map identifies the public boundary and adjacent owners.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
