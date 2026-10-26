# crates/app/testing/peritus-platform-qualification/src/manifest — navigation

## Responsibility

Implementation pieces for `manifest.rs`: Deterministic package manifests and checksum material.

## Files in this folder

| File | Role |
| --- | --- |
| [wire.rs](wire.rs) | Strict deserialization models for canonical package-manifest TOML. |

Owning module: [manifest.rs](../manifest.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
