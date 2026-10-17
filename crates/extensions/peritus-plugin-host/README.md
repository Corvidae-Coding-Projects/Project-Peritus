# peritus-plugin-host

## Folder guide

Isolated process and Wasm plugin discovery, trust, lifecycle, and quota host.

Active root-workspace package; declarations and targets are in [Cargo.toml](Cargo.toml).

Use [AGENTS.md](AGENTS.md) for file-level responsibilities, child modules, parent context, and nearby implementation owners.

### Local files

| File | Role |
| --- | --- |
| [Cargo.toml](Cargo.toml) | Cargo package/dependency/target declarations for `peritus-plugin-host`. |
| [README.md](README.md) | Human guide and operating/background notes for this folder; AGENTS.md provides the local navigation map. |

### Subfolders

| Folder | Role / next waypoint |
| --- | --- |
| [src/](src/AGENTS.md) | Host for untrusted out-of-process and Wasm Peritus plugins. |

Discovers strict plugin manifests, verifies exact manifest/artifact trust, launches executable or
WASI-compatible Wasm plugins outside the daemon, negotiates the versioned SDK protocol, and owns
bounded request lifecycle, quotas, cancellation, shutdown, and failure classification. Authority
is supplied by a G0/B1 mediator; the host cannot mint or widen it.

## Behavioral work

Use the [project real-testing policy](../../../README.md#real-testing) and the actual interfaces owned by this crate. The [local map](AGENTS.md) identifies entrypoints and implementation owners.

Process ownership is not an OS sandbox: process plugins can use their OS account's
ambient filesystem/network authority. This API is separate from the declarative
product module registry. It uses owned process groups/jobs for cleanup, cancellation
and deadlines across both writes and reads, fresh authorization after queued
admission, and explicit stop/start recovery. A possibly sent request is never
automatically replayed; reconcile its external effects before retrying.
