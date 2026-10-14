# crates/tools/peritus-tools-fs/src — navigation

## Responsibility

Bounded filesystem tools for immutable and authorized Peritus workspaces.

## Files in this folder

| File | Role |
| --- | --- |
| [catalog.rs](catalog.rs) | Canonical filesystem tool descriptors. |
| [decoder.rs](decoder.rs) | Strict schema-validated filesystem argument projection. |
| [dispatcher.rs](dispatcher.rs) | Router-authorized filesystem dispatcher adapters. |
| [error.rs](error.rs) | Stable filesystem-tool failures and recovery guidance. |
| [input.rs](input.rs) | Checked bounded filesystem-tool input values. |
| [lib.rs](lib.rs) | Bounded filesystem tools for immutable and authorized Peritus workspaces. |
| [mutation.rs](mutation.rs) | Inert filesystem mutation compilation. |
| [read.rs](read.rs) | Bounded immutable filesystem observations. |
| [read_digest.rs](read_digest.rs) | Streaming digests over complete filesystem observations. |
| [render.rs](render.rs) | Bounded model, human, and structured filesystem renderings. |
| [schemas.rs](schemas.rs) | Exact version-one filesystem schemas. |
| [verified.rs](verified.rs) | Executable invariant/refinement predicates for the local domain. Entry names: `traversal_bounds_valid`. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [dispatcher/](dispatcher/AGENTS.md) | Implementation pieces for `dispatcher.rs`: Router-authorized filesystem dispatcher adapters. |
| [input/](input/AGENTS.md) | Implementation pieces for `input.rs`: Checked bounded filesystem-tool input values. |
| [read/](read/AGENTS.md) | Implementation pieces for `read.rs`: Bounded immutable filesystem observations. |
| [render/](render/AGENTS.md) | Implementation pieces for `render.rs`: Bounded model, human, and structured filesystem renderings. |

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
