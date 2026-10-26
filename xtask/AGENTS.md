# xtask — navigation

## Responsibility

Repository packaging and release command owner. Read the current command dispatcher rather than old qualification instructions.

Package `xtask` is a member of the root Cargo workspace; architectural owner `A0` in `engineering`.

## Files in this folder

| File | Role |
| --- | --- |
| [Cargo.toml](Cargo.toml) | Cargo package/dependency/target declarations for `xtask`. |
| [README.md](README.md) | Human guide and operating/background notes for this folder; AGENTS.md provides the local navigation map. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [src/](src/AGENTS.md) | Packaging, installation, distribution, and release tools for Peritus. |

## Working in this area

Use [`README.md`](README.md) to choose the local entry point, then follow the parent waypoint to understand callers, ownership, and neighboring systems. Inspect the referenced file and current call sites before changing behavior, and update this map whenever files or responsibilities move.

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
