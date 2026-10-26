# release — navigation

## Responsibility

Release contract/configuration assets and candidate-specific publication records.

## Files in this folder

| File | Role |
| --- | --- |
| [README.md](README.md) | Human guide and operating/background notes for this folder; AGENTS.md provides the local navigation map. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [assets/](assets/AGENTS.md) | README describing externally generated candidate release assets; this source directory contains no release binaries. |
| [schemas/](schemas/AGENTS.md) | Versioned artifact, provenance, readiness, and release-evidence JSON contracts. |
| [templates/](templates/AGENTS.md) | Inert release/evidence templates. Instantiating a template does not establish candidate readiness. |

## Working in this area

Use [`README.md`](README.md) to choose the local entry point, then follow the parent waypoint to understand callers, ownership, and neighboring systems. Inspect the referenced file and current call sites before changing behavior, and update this map whenever files or responsibilities move.

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
