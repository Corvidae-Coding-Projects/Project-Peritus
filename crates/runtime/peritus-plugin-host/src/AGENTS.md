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

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
