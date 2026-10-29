# crates/runtime/peritus-plugin-host/src — navigation

## Responsibility

Host for untrusted out-of-process and Wasm Peritus plugins.

## Files in this folder

| File | Role |
| --- | --- |
| [authority.rs](authority.rs) | G0/B1 authority mediation hook for untrusted plugin invocations. |
| [cancellation.rs](cancellation.rs) | Race-safe cooperative host cancellation. |
| [discovery.rs](discovery.rs) | Bounded filesystem discovery and exact artifact identification. |
| [error.rs](error.rs) | Stable host failure classification. |
| [host.rs](host.rs) | Plugin host lifecycle and authority-bound invocation orchestration. |
| [lib.rs](lib.rs) | Host for untrusted out-of-process and Wasm Peritus plugins. |
| [quota.rs](quota.rs) | Atomic lifecycle and concurrent-invocation quota accounting. |
| [transport.rs](transport.rs) | Owned process/Wasm subprocess and bounded framed exchange. |
| [trust.rs](trust.rs) | Explicit exact-digest plugin trust policy. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [transport/](transport/AGENTS.md) | Native-plugin child ownership, framed transport I/O, bounded diagnostics, deadline handling, cancellation, and joined process cleanup. |

## Working in this area

Start at [`lib.rs`](lib.rs) for the module surface, then follow its declarations into the focused files and child waypoints above. Trace at least one caller and the relevant error or recovery path before moving responsibilities; the parent package map identifies the public boundary and adjacent owners.

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
