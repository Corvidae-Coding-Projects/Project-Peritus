# peritus-app-protocol

`peritus-app-protocol` is the transport-neutral A3 contract shared by future Peritus daemon, CLI,
TUI, and extension clients. It defines bounded version negotiation, typed application envelopes,
exact B3 command and event frame bindings, resumable at-least-once subscriptions, artifact transfer,
prompt correlation, terminal streaming, daemon controls, stable errors, deterministic schemas, and
compatibility fixtures.

The crate does not open sockets or named pipes, authenticate peers, access storage, supervise
processes, or grant domain authority. Those effects belong to G0 and its B0/B1/C0/C2 dependencies.

The complete contract and verification plan is in
[`../../../.design/a3-app-protocol.md`](../../../.design/a3-app-protocol.md).

## Focused checks

From the repository root:

```sh
CARGO_BUILD_JOBS=2 cargo test --locked --package peritus-app-protocol
```
