# G3 Extensions and MCP

G3 provides three H-class crates: `peritus-plugin-sdk`, `peritus-plugin-host`, and `peritus-mcp`.
Together they define canonical external-extension contracts, execute plugins outside the daemon
address space, and project selected Peritus capabilities through MCP. They are authority consumers,
not authority issuers.

## Plugin manifest and wire contract

Each immediate plugin directory beneath a configured discovery root contains
`peritus-plugin.toml` and one relative regular-file artifact. Schema version one declares:

- canonical plugin identity and semantic version;
- `process` or `wasm-component` isolation kind;
- an inclusive protocol compatibility range;
- a traversal-free relative artifact and literal startup arguments;
- strictly ordered unique hierarchical capability names and their operation classes;
- positive concurrency, frame, output, invocation-time, lifecycle-request, and violation quotas;
- optional detached signature metadata interpreted by the configured trust verifier.

Unknown fields, absolute or traversing artifacts, unordered/duplicate capabilities, unsupported
protocols, control-containing values, and zero/oversized collections fail closed. The SDK produces
deterministic canonical JSON and SHA-256 manifest identities. Its trust preimage domain-separates
the unsigned canonical manifest and exact artifact SHA-256 so signatures bind both policy and code.

Host/plugin traffic uses a versioned length-delimited canonical JSON envelope with stable request,
invocation, and failure identities. Initialization, invocation, cancellation, status, and shutdown
are explicit protocol operations. A decoded request never carries an executable B1 capability or
C4 permit.

## Discovery, trust, and isolation

Discovery is bounded by root count, plugin count, manifest bytes, and artifact bytes. Roots,
plugin directories, manifests, and artifacts may not be symlinks. Canonical artifact paths must
remain beneath the canonical plugin directory. Duplicate identity/version pairs are rejected, and
the host records exact manifest and artifact digests.

Starting a plugin rechecks configured trust, intersects every requested quota with the host ceiling,
launches one isolated child, negotiates protocol version one, and requires a Ready response before
admission. Concurrent starts of the same identity serialize through the host-owned start gate.

`process` executes the artifact directly with literal arguments and framed standard input/output.
`wasm-component` invokes the absolute configured Wasmtime executable with the component artifact
and literal arguments. The crate does not download a runtime or infer one from an ambient plugin
directory. OS-level confinement remains the responsibility of the packaged G0/C3 service launch;
the host still enforces protocol, authority, lifecycle, and resource ceilings.

Every invocation names one declared capability and exact subject. The `AuthorityMediator` asks the
daemon-owned B1/C4 boundary for a current narrow grant. Denial occurs before an invocation frame is
sent. The host validates returned grants, accounts concurrency/lifecycle/output quotas, propagates
cancellation, applies wall-clock limits, classifies protocol/transport/plugin failures, and owns
graceful shutdown or forced termination. A crashed child moves to a failed state and is not
silently treated as a successful or reusable instance.

## MCP server

`peritus-mcp` implements newline-delimited MCP JSON-RPC for protocol version `2025-06-18`. An
embedding constructs `McpServer` with server identity, optional instructions, one authenticated
`BridgeContext`, an `AuthorityBridge`, and positive limits for message bytes, active requests, and
page entries.

The lifecycle requires `initialize`, then the `notifications/initialized` notification, before the
normal request surface. Supported operations are:

- `ping`;
- `tools/list` and `tools/call`;
- `resources/list` and `resources/read`;
- `prompts/list` and `prompts/get`;
- `notifications/cancelled` for active request cancellation.

Lists are deterministically paginated with opaque bounded cursors. Duplicate active JSON-RPC IDs
are rejected. Initialize is serialized, request admission uses nonblocking semaphore acquisition so
the reader remains available for cancellation, and all request/writer tasks are owned, joined, or
aborted during malformed input and transport shutdown.

`AuthorityBridge` lists only tools already exposed by the current C4 registry and B1 scope, routes
calls through authoritative C4 preparation/authorization/dispatch, and resolves resources/prompts
under the exact A3 session and revision. C4 result status is preserved: non-success results set the
MCP error indication rather than being rendered as successful prose.

## Daemon embedding boundary

The three crates are production implementation surfaces, but G3 intentionally does not fabricate
the missing facts required to authorize an effect. `BridgeContext` identifies actor, session, and
authority generation; a real tool invocation also requires the exact run, workspace, target,
capability scope, and current C4 lifecycle data owned by G0/B1. Therefore the H0 packaged
application composition must provide the concrete `AuthorityMediator` and `AuthorityBridge` at the
point where those facts exist. A permissive fallback adapter would violate the authority model and
is not provided.

## Conformance and verification

A2's runtime-neutral plugin suite contains seven cases: canonical manifest, trust required,
authority denial produces no effect, lifecycle ordering, quota enforcement, cancellation, and
crash isolation. `peritus-plugin-host` runs the suite against real isolated fixture processes,
including host/plugin framing and termination.

```text
CARGO_BUILD_JOBS=1 cargo test --locked \
  --package peritus-plugin-sdk --package peritus-plugin-host --package peritus-mcp \
  --package peritus-conformance --all-targets --all-features
CARGO_BUILD_JOBS=1 cargo clippy --locked \
  --package peritus-plugin-sdk --package peritus-plugin-host --package peritus-mcp \
  --package peritus-conformance --all-targets --all-features -- -D warnings
cargo verus verify --package peritus-plugin-sdk --package peritus-plugin-host \
  --package peritus-mcp --all-features --locked --check-toolchain \
  --fwd-verus-args-to roots -- --no-cheating --rlimit 20
```

The repository Gate A commands contain these H-class packages in their complete reviewed package
inventory; the focused Verus command above is diagnostic evidence, not a substitute for Gate A.
