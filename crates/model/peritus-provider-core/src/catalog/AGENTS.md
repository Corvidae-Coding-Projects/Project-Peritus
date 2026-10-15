# crates/model/peritus-provider-core/src/catalog — navigation

## Responsibility

Implementation pieces for `catalog.rs`: Provider-owned model discovery; no bundled model names or implicit fallback catalog.

## Files in this folder

| File | Role |
| --- | --- |
| [http.rs](http.rs) | Bounded authenticated GET discovery with same-origin token pagination. |
| [parse.rs](parse.rs) | Strict bounded catalog projections shared by HTTP and official account protocols. |
| [runtime.rs](runtime.rs) | Bounded official account-runtime metadata queries, with no inference or credential custody. |

Owning module: [catalog.rs](../catalog.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
