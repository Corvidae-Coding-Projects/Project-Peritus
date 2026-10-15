# crates/state/peritus-approval/src/render — navigation

## Responsibility

Implementation pieces for `render.rs`: Deterministic bounded ASCII rendering of typed approval facts.

## Files in this folder

| File | Role |
| --- | --- |
| [builder.rs](builder.rs) | Capacity-aware whole-field rendering with strict raw-byte validation. |
| [collections.rs](collections.rs) | Whole-item rendering and exact omission accounting for bounded collections. |
| [encoding.rs](encoding.rs) | Printable names and allocation-bounded integer/hex encodings. |
| [facts.rs](facts.rs) | Mandatory scalar facts that must never be silently omitted. |

Owning module: [render.rs](../render.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
