# crates/state/peritus-journal/src/domain/kernel — navigation

## Responsibility

Implementation pieces for `kernel.rs`: B0 durable-transition adapter and reducer-driven restart replay.

## Files in this folder

| File | Role |
| --- | --- |
| [capsule.rs](capsule.rs) | Canonical replay-capsule encoding and bounded decoding. |
| [commit.rs](commit.rs) | Move-only B0 commit requests and post-commit observations. |
| [recovery.rs](recovery.rs) | Reducer-driven reconstruction of durable B0 aggregate chains. |
| [state_digest.rs](state_digest.rs) | Canonical digest of the complete B0 successor aggregate. |

Owning module: [kernel.rs](../kernel.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../AGENTS.md) · [Repository map](../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
