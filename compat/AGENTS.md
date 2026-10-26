# compat — navigation

## Responsibility

Versioned protocol compatibility data. Surface/version/case directories pair manifests, input bytes, and expected decoder outcomes.

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [app-protocol/](app-protocol/AGENTS.md) | Compatibility data for `app-protocol`. Descend to a named case for its manifest, bytes and expected result. |

## Nearby owners

- [Application wire owner](../crates/app/peritus-app-protocol/AGENTS.md).
- [Domain wire owner](../crates/foundation/peritus-protocol/AGENTS.md).
- [Model wire owner](../crates/model/peritus-model-protocol/AGENTS.md).

## Working in this area

Use the responsibility and tables above to choose the local entry point, then follow the parent waypoint to understand callers, ownership, and neighboring systems. Inspect the referenced file and current call sites before changing behavior, and update this map whenever files or responsibilities move.

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
