# peritus-tools-shell

## Folder guide

Authorized structured argv and explicit script tools for Peritus.

Active root-workspace package; declarations and targets are in [Cargo.toml](Cargo.toml).

Use [AGENTS.md](AGENTS.md) for file-level responsibilities, child modules, parent context, and nearby implementation owners.

### Local files

| File | Role |
| --- | --- |
| [Cargo.toml](Cargo.toml) | Cargo package/dependency/target declarations for `peritus-tools-shell`. |
| [README.md](README.md) | Human guide and operating/background notes for this folder; AGENTS.md provides the local navigation map. |

### Subfolders

| Folder | Role / next waypoint |
| --- | --- |
| [src/](src/AGENTS.md) | Authorized structured argv and explicit script tool adapters. |

`peritus-tools-shell` supplies the built-in `shell.exec` and `shell.script` descriptors and their
authorized C2/C3 dispatch adapters. Structured argv is the default. Script text is accepted only
by the separately named, higher-risk script descriptor.

Both tools require a precompiled restricted `ExecutionPlan`, matching C2 authority, a checked C3
sandbox plan and admission, and a concrete native backend. There is no unrestricted execution
fallback. Running processes remain owned by C2 while this crate maps their ordered observations,
controls, artifacts, terminal states, and recovery into C4 envelopes.

`ShellDispatcher` is the restricted production adapter. `RawShellDispatcher` serves callers that
have already compiled and authorized an explicit raw-effect plan, including G4's workspace-scoped
developer commands. It does not weaken the default descriptor or create a separate process path:
both dispatchers launch through `ExecutionGateway`, retain output in the artifact store, and expose
the same C4 poll, stdin, resize, signal, cancel, and recovery protocol.

## Behavioral work

Use the [project real-testing policy](../../../README.md#real-testing) and the actual interfaces owned by this crate. The [local map](AGENTS.md) identifies entrypoints and implementation owners.
