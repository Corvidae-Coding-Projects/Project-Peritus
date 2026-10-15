# peritus-web

## Folder guide

Local browser workspace and gateway for the Peritus harness.

Active root-workspace package; declarations and targets are in [Cargo.toml](Cargo.toml).

Use [AGENTS.md](AGENTS.md) for file-level responsibilities, child modules, parent context, and nearby implementation owners.

### Local files

| File | Role |
| --- | --- |
| [Cargo.toml](Cargo.toml) | Cargo package/dependency/target declarations for `peritus-web`. |
| [README.md](README.md) | Human guide and operating/background notes for this folder; AGENTS.md provides the local navigation map. |

### Subfolders

| Folder | Role / next waypoint |
| --- | --- |
| [src/](src/AGENTS.md) | Loopback HTTP gateway, browser workspace/session service, CLI PTY console bridge, static assets, and daemon-facing routes. |

Local-only Rust gateway for the [Peritus WebUI](../../../webui/README.md).
Axum serves the prebuilt Svelte assets and same-origin HTTP API. The existing
`peritus-app-client` connects to the daemon; a retained PTY exposes the installed
CLI for interactive setup and advanced workflows.

```sh
cargo build -p peritus-web -j 2
./target/debug/peritus-web --assets webui/dist --root /path/to/project
```

Build frontend assets first as described in the WebUI guide. Production runtime
requires the Rust binary, built assets, and installed Peritus/Git, without Node.
Use `--help` for path/endpoint overrides.

## Boundaries

- `api.rs`: loopback host/origin/token checks, HTTP routes, shared mutation dispatcher.
- `api/file_response.rs`: confined streaming, PDF-only inline policy, and attachment delivery.
- `server.rs`: local listener startup and graceful shutdown.
- `state.rs`: durable presentation state, exact prepared operation context and outcomes, canonical-root nesting.
- `config.rs`: validated TOML and platform-native discovery independent of WebUI overrides.
- `daemon.rs`: native conversation, model, status, and run protocol adapter.
- `files.rs`: project-confined directory pages and UTF-8 text previews.
- `git.rs`: explicit repository binding and real argument-vector Git operations.
- `processes.rs`: exact durable command bindings, retained spool ranges, completion and cancellation.
- `terminal.rs`: installed CLI console input, resize, output ranges, and termination.

Agent execution and product policy remain daemon-owned. Native and CLI
capability routing, recovery behavior, limits, and verification scope are
documented in the [WebUI guide](../../../webui/README.md) and
[command coverage](../../../webui/COMMANDS.md).

## Behavioral work

Use the [project real-testing policy](../../../README.md#real-testing) and the actual interfaces owned by this crate. The [local map](AGENTS.md) identifies entrypoints and implementation owners.
