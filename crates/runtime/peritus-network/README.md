# peritus-network

## Folder guide

outbound connection planning and bounded managed proxy.

Active root-workspace package; declarations and targets are in [Cargo.toml](Cargo.toml).

Use [AGENTS.md](AGENTS.md) for file-level responsibilities, child modules, parent context, and nearby implementation owners.

### Local files

| File | Role |
| --- | --- |
| [Cargo.toml](Cargo.toml) | Cargo package/dependency/target declarations for `peritus-network`. |
| [README.md](README.md) | Human guide and operating/background notes for this folder; AGENTS.md provides the local navigation map. |

### Subfolders

| Folder | Role / next waypoint |
| --- | --- |
| [src/](src/AGENTS.md) | Connection planning and bounded managed outbound proxy. |

`peritus-network` narrows a checked C2 network contract into a digest-bound runtime plan and owns a
bounded loopback HTTP/CONNECT proxy. It rechecks names, resolved addresses, redirects, limits, and
credential scope. The crate grants no authority and never interprets a proxy decision as run
acceptance.

The proxy is deny-by-default, has bounded workers and observations, and joins owned work during
shutdown. Tests use controlled loopback servers; production code does not require public Internet
access.

## Behavioral work

Use the [project real-testing policy](../../../README.md#real-testing) and the actual interfaces owned by this crate. The [local map](AGENTS.md) identifies entrypoints and implementation owners.
