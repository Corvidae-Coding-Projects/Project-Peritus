# peritus-web

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
- `state.rs`: durable presentation state, original operation outcomes, canonical-root nesting.
- `config.rs`: validated TOML and platform-native discovery independent of WebUI overrides.
- `daemon.rs`: native conversation, model, status, and run protocol adapter.
- `files.rs`: project-confined directory pages and UTF-8 text previews.
- `git.rs`: explicit repository binding and real argument-vector Git operations.
- `terminal.rs`: retained installed CLI processes, bounded output, input/resize/termination.

Agent execution and product policy remain daemon-owned. Native and CLI
capability routing, recovery behavior, limits, and verification scope are
documented in the [WebUI guide](../../../webui/README.md) and
[command coverage](../../../webui/COMMANDS.md).

## Focused checks

```sh
cargo test --locked -p peritus-web -j 2
cargo fmt -p peritus-web -- --check
cargo clippy --locked -p peritus-web --all-targets --no-deps -j 2 -- -D warnings
```

The Unix socket fixture makes no provider calls. HTTP/browser and real Git/CLI
integration tests live in `webui/tests/e2e` and run against isolated temporary
projects on port 4174.
