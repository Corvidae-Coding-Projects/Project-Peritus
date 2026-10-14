# peritus-app-protocol

## Folder guide

Transport-neutral application protocol contract for Peritus clients and daemons.

Active root-workspace package; declarations and targets are in [Cargo.toml](Cargo.toml).

Use [AGENTS.md](AGENTS.md) for file-level responsibilities, child modules, parent context, and nearby implementation owners.

### Local files

| File | Role |
| --- | --- |
| [Cargo.toml](Cargo.toml) | Cargo package/dependency/target declarations for `peritus-app-protocol`. |
| [README.md](README.md) | Human guide and operating/background notes for this folder; AGENTS.md provides the local navigation map. |

### Subfolders

| Folder | Role / next waypoint |
| --- | --- |
| [src/](src/AGENTS.md) | Transport-neutral application protocol for Peritus. |

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

The complete contract and verification plan is in
[`../../../.design/a3-app-protocol.md`](../../../.design/a3-app-protocol.md).

## Behavioral work

Use the [project real-testing policy](../../../README.md#real-testing) and the actual interfaces owned by this crate. The [local map](AGENTS.md) identifies entrypoints and implementation owners.
