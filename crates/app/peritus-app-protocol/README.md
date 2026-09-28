# peritus-app-protocol

`peritus-app-protocol` is the transport-neutral A3 contract shared by future Peritus daemon, CLI,
TUI, and extension clients. It defines bounded version negotiation, typed application envelopes,
exact B3 command and event frame bindings, resumable at-least-once subscriptions, artifact transfer,
prompt correlation, terminal streaming, daemon controls, verified candidate settlement, stable
errors, deterministic schemas, and compatibility fixtures.

The optional `app.terminal-failure` feature adds event tag 15, `TerminalUnavailable`, containing
the exact `TerminalBinding`. It ends delivery for that attachment without asserting process exit,
cancellation, or loss of the application connection. Existing event tags and bytes are unchanged.
Servers send this event only to clients that negotiated the feature; older clients receive a
bounded diagnostic instead.

The optional `app.terminal-pipes` feature adds response tag 181, `TerminalPipeAttached`, carrying
the exact attachment binding. It identifies an existing interactive preview with bounded stdin,
stdout, and stderr pipes instead of a PTY. Clients must negotiate it before such an attachment
is admitted. Stream offsets remain independent and event sequencing remains global; pipe mode
does not grant resize support or new process-launch authority. `TerminalAttached` retains its
existing PTY meaning and wire bytes.

Product responses preserve the legacy `ProductRunSnapshot` bytes and add append-only settlement
payload tags. A settlement identifies the exact candidate and conversation revision, distinguishes
automated qualification from the user's existing `ProductDeliverable::accepted` choice, and reports
candidate work honestly even when a provider, gate, review, or adapter stops before acceptance.
Legacy deliverables decode as qualified; new partial candidates use only the settlement payloads.

The optional `app.workbench-run-binding` feature adds request tag 45 and response tag 44 for
authenticated discovery of a run's durable conversation. A binding contains the existing
interaction snapshot and its exact workbench destination, or no destination for a legacy
interactive run. The effort-presence flag precedes the snapshot; legacy message bytes and tags
remain unchanged. Noninteractive coding runs retain the existing `InvalidState` response
(`IdempotencyConflict` on the wire) and coding-run follow-up flow. Other lookup errors must
retain the selected run for retry rather than imply that it is a different kind of run.

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
