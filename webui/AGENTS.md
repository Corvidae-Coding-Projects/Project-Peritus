# webui — navigation

## Responsibility

Svelte browser client and its build/static assets. The Rust gateway is peritus-web; the daemon owns agent execution.

## Files in this folder

| File | Role |
| --- | --- |
| [COMMANDS.md](COMMANDS.md) | Document: Command and capability routing. |
| [DAEMON-INVESTIGATION.md](DAEMON-INVESTIGATION.md) | Document: Local daemon investigation — 2026-09-12. |
| [README.md](README.md) | Human guide and operating/background notes for this folder; AGENTS.md provides the local navigation map. |
| [SECURITY.md](SECURITY.md) | Document: Experimental WebUI security boundaries. |
| [THIRD-PARTY.md](THIRD-PARTY.md) | Document: Frontend third-party notices. |
| [index.html](index.html) | Browser HTML mount document loaded by the frontend entrypoint. |
| [package-lock.json](package-lock.json) | Locked dependency resolution for the neighboring package manifest. |
| [package.json](package.json) | Node package metadata, dependencies and scripts for `peritus-webui`. |
| [svelte.config.js](svelte.config.js) | Svelte compiler/preprocessor configuration for the browser client. |
| [tsconfig.json](tsconfig.json) | TypeScript compiler and source-inclusion settings for the browser client. |
| [vite.config.ts](vite.config.ts) | Vite build/dev-server configuration and API proxy routing to the Rust gateway. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [public/](public/AGENTS.md) | Static files copied into the browser build, including source-controlled notices and other public assets. |
| [scripts/](scripts/AGENTS.md) | Frontend inspection utility for observing the real browser interface and retaining screenshots/interaction observations. |
| [src/](src/AGENTS.md) | Browser UI entrypoint, application composition, and client-side modules. |

## Nearby owners

- [HTTP gateway and CLI console bridge](../crates/app/peritus-web/AGENTS.md).
- [Daemon-owned agent work](../crates/app/peritus-daemon/src/product_run/AGENTS.md).

## Working in this area

Start with [`package.json`](package.json), then trace UI state through the typed API boundary to the Rust gateway and daemon owner before changing behavior. Keep presentation-only state separate from durable run authority, and update the local component or file route when the browser surface moves.

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
