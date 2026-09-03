# peritus-app-protocol

`peritus-app-protocol` is the transport-neutral A3 contract shared by future Peritus daemon, CLI,
TUI, and extension clients. It defines bounded version negotiation, typed application envelopes,
exact B3 command and event frame bindings, resumable at-least-once subscriptions, artifact transfer,
prompt correlation, terminal streaming, daemon controls, verified candidate settlement, stable
errors, deterministic schemas, and compatibility fixtures.

Product responses preserve the legacy `ProductRunSnapshot` bytes and add append-only settlement
payload tags. A settlement identifies the exact candidate and conversation revision, distinguishes
automated qualification from the user's existing `ProductDeliverable::accepted` choice, and reports
candidate work honestly even when a provider, gate, review, or adapter stops before acceptance.
Legacy deliverables decode as qualified; new partial candidates use only the settlement payloads.

The crate does not open sockets or named pipes, authenticate peers, access storage, supervise
processes, or grant domain authority. Those effects belong to G0 and its B0/B1/C0/C2 dependencies.

The complete contract and verification plan is in
[`../../../.design/a3-app-protocol.md`](../../../.design/a3-app-protocol.md).

## Focused checks

From the repository root:

```sh
CARGO_BUILD_JOBS=2 cargo test --locked --package peritus-app-protocol
CARGO_BUILD_JOBS=1 cargo run --locked --package peritus-app-protocol \
  --bin peritus-app-protocol-codegen -- --root . --check
```
