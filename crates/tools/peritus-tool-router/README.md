# peritus-tool-router

## Folder guide

Exact-authority tool registry, router, dispatch, control, and replay boundary.

Active root-workspace package; declarations and targets are in [Cargo.toml](Cargo.toml).

Use [AGENTS.md](AGENTS.md) for file-level responsibilities, child modules, parent context, and nearby implementation owners.

### Local files

| File | Role |
| --- | --- |
| [Cargo.toml](Cargo.toml) | Cargo package/dependency/target declarations for `peritus-tool-router`. |
| [README.md](README.md) | Human guide and operating/background notes for this folder; AGENTS.md provides the local navigation map. |

### Subfolders

| Folder | Role / next waypoint |
| --- | --- |
| [src/](src/AGENTS.md) | Exact-authority C4 tool router and sole invocation-permit owner. |

`peritus-tool-router` is the control point between an agent's requested tool call and a concrete
tool implementation. It registers available tools, exposes only the capabilities permitted for the
current role, prepares calls, checks the exact authority granted by policy, and records enough
bounded state to replay or recover a dispatch safely.

The router sits above the tool protocol and below orchestration. It does not execute shell, Git, or
filesystem operations itself, and it cannot invent permissions. Execution stays in the dedicated
tool crates; authority stays in the policy and approval layers.

Registration, preparation, authorization, dispatch permits, and replay controls remain separate so
providers and tools can evolve without turning the router into a monolithic executor.

## Behavioral work

Use the [project real-testing policy](../../../README.md#real-testing) and the actual interfaces owned by this crate. The [local map](AGENTS.md) identifies entrypoints and implementation owners.
