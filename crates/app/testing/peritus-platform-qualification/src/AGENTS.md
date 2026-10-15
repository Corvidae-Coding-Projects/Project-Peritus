# crates/app/testing/peritus-platform-qualification/src — navigation

## Responsibility

H2 packaged-host qualification contracts for Linux, macOS, and Windows.

## Files in this folder

| File | Role |
| --- | --- |
| [assets.rs](assets.rs) | Reviewed native packaging assets embedded for release-builder inspection. |
| [digest.rs](digest.rs) | Deterministic SHA-256 artifact identities. |
| [equivalence.rs](equivalence.rs) | Repository-versus-package process-equivalence contracts. |
| [error.rs](error.rs) | Stable H2 error classification. |
| [evidence.rs](evidence.rs) | Bounded, secret-free H2 evidence values. |
| [layout.rs](layout.rs) | Concrete cross-target release layouts and ownership contracts. |
| [lib.rs](lib.rs) | H2 packaged-host qualification contracts for Linux, macOS, and Windows. |
| [lifecycle.rs](lifecycle.rs) | Installer, upgrade, rollback, and uninstall ownership plans. |
| [manifest.rs](manifest.rs) | Deterministic package manifests and checksum material. |
| [package_builder.rs](package_builder.rs) | Host-native package assembly behind the thin `peritus-package` binary. |
| [platform.rs](platform.rs) | Validated packaged-host platform contracts and declared deltas. |
| [sandbox.rs](sandbox.rs) | Native C3 sandbox qualification contracts and observations. |
| [service.rs](service.rs) | Optional per-user supervisor contracts for an explicitly enabled foreground G0 daemon. |
| [transport.rs](transport.rs) | Exact packaged G0 local-transport expectations. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [bin/](bin/AGENTS.md) | Executable entrypoints for `peritus-platform-qualification`; follow each binary into its package implementation. |
| [layout/](layout/AGENTS.md) | Implementation pieces for `layout.rs`: Concrete cross-target release layouts and ownership contracts. |
| [manifest/](manifest/AGENTS.md) | Implementation pieces for `manifest.rs`: Deterministic package manifests and checksum material. |
| [platform/](platform/AGENTS.md) | Implementation pieces for `platform.rs`: Validated packaged-host platform contracts and declared deltas. |

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
