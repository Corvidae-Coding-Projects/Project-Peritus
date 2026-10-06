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

Product responses expose the current `ProductRunSnapshot` and settlement payloads. A settlement
identifies the exact candidate and conversation revision, distinguishes automated qualification
from the user's `ProductDeliverable::accepted` choice, and reports candidate work honestly even
when a provider, gate, review, or adapter stops before acceptance. Earlier product protocol shapes
are rejected rather than decoded or migrated.

The `app.workbench-run-binding` feature uses request tag 45 and response tag 44 for authenticated
discovery of a run's durable conversation. Every binding contains the interaction snapshot and its
exact workbench destination. Runs without that binding are not admitted into the current product
flow. Lookup errors retain the selected run for retry rather than implying a different run kind.

The crate does not open sockets or named pipes, authenticate peers, access storage, supervise
processes, or grant domain authority. Those effects belong to G0 and its B0/B1/C0/C2 dependencies.

File path descriptors and retained source labels have no separate byte allowance or portable
control-character restriction. They retain exact non-NUL UTF-8 text; the host's platform-aware
path validator and handle-relative read own filesystem safety, while display surfaces escape
labels. Preview and page decoders apply the independent codec transport contract. Historical
wire bytes remain exact. C0 retains historical file-source JSON exactly and uses a distinct,
platform-bound native origin for newly accepted paths; older persistence readers must be upgraded
before those sources are published. Attachment size, history and context policies remain separate.

The complete contract and verification plan is in
[`../../../.design/a3-app-protocol.md`](../../../.design/a3-app-protocol.md).

## Focused checks

From the repository root:

```sh
CARGO_BUILD_JOBS=2 cargo test --locked --package peritus-app-protocol
CARGO_BUILD_JOBS=1 cargo run --locked --package peritus-app-protocol \
  --bin peritus-app-protocol-codegen -- --root . --check
```
