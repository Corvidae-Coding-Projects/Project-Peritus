# crates/runtime/peritus-workspace/src/task_candidate/managed/transaction — navigation

## Responsibility

Durable explicit discard recovery; inspection never publishes restore effects.

## Files in this folder

| File | Role |
| --- | --- |
| [mod.rs](mod.rs) | Durable explicit discard recovery; inspection never publishes restore effects. |
| [owned.rs](owned.rs) | Nonces distinguish our preparation from foreign Git locks and adjacent user files. |
| [state.rs](state.rs) | Bounded, checksummed restore intent and progress, separate from source effects. |
| [validation.rs](validation.rs) | Validate recorded transitions before retrying any source, index, HEAD, or archive effect. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [validation/](validation/AGENTS.md) | Implementation pieces for `validation.rs`: Validate recorded transitions before retrying any source, index, HEAD, or archive effect. |

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../../AGENTS.md) · [Repository map](../../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
