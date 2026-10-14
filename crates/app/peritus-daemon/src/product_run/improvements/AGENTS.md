# crates/app/peritus-daemon/src/product_run/improvements — navigation

## Responsibility

Implementation pieces for `improvements.rs`: Collect inert suggestions from terminal runs; inference starts only on explicit evaluation.

## Files in this folder

| File | Role |
| --- | --- |
| [evaluation.rs](evaluation.rs) | Durable workbench preparation for one explicitly selected harness improvement. |
| [store.rs](store.rs) | Transactional local inbox. Suggestions carry no execution or promotion authority. |

Owning module: [improvements.rs](../improvements.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../AGENTS.md) · [Repository map](../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
