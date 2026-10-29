# Native Rust plugin bridge

## Responsibility

[native.rs](../native.rs) drives generic plugin actions; feature behavior belongs to the independently compiled module.

| File | Responsibility |
| --- | --- |
| [runtime.rs](runtime.rs) | Enabled-digest launch snapshot, existing native process host, per-call revision-bound live authority, framed service RPC, exact writer/reviewer route metadata, and cancellation/cleanup. |
| [scoped_source.rs](scoped_source.rs) | Explicit current-workspace capture, immutable content-addressed manifests, revocable handles, mandatory Git/Peritus metadata exclusions, bounded range evidence, exact before/after byte verification, reuse and corruption fail-closure. |
| [application_laboratory.rs](application_laboratory.rs) | Generic Linux disposable-source lifecycle, literal-argv execution, isolated module-worker execution, bounded raw process observations, cancellation, cleanup, and original-source verification. It contains no module-specific adapters or result semantics. |
| [tools.rs](tools.rs) | Borrowed host service intersection and plugin-exported model functions; actual observation delivery. |
| [publication.rs](publication.rs) | Shared enabled-package validation and exact-run actions, concrete progress, bounded summary, and export presentation for both clients. |

[Parent map](../AGENTS.md). Native code runs under the OS account; host-service mediation is not an OS sandbox.

A successful plugin function may return `request_complete: true` to yield immediately to its
coordinator after its durable checkpoint. Ordinary host web observations cannot trigger this.
The coordinator validates the saved output before advancing; this is not an approval bypass.

While a native module role is active, the host may durably admit only that same module's `pause`,
`resume`, or nonempty `steer` control. The current model request and its tools remain bound to their
old revision; after cancellation, the coordinator explicitly rebinds the next plugin call to the
newly incorporated control revision. Other input retains the ordinary supersession behavior.

`Checkpoint` publishes conversation Markdown before `checkpoint_ack` continues the plugin.
Cancellation or new user input prevents acknowledgement. A crash between publication and
acknowledgement can replay that checkpoint; this contract favors retained work over losing it.

Every module call receives the additive versioned host capability profile. `HostService` requests
are returned through `host_result`; unsupported source, worker, laboratory, or source-build services are explicit
outcomes and are never treated as permission to fall back to ambient effects.

On configured Linux hosts, `module.worker@1` executes the selected module's own named worker from
the exact enabled package in a disposable copy of explicit scoped source. The host interprets no
worker name, adapter, vulnerability class, probe, oracle, finding, or repair policy. It narrows
generic resource bounds, clears credentials and networking, owns the process tree, returns bounded
raw streams and lifecycle facts, removes disposable bytes, and verifies the original source.
Module-specific code and typed observations remain in the separately installed module.
Workers may request a small named set of generic runtime resources. The browser-automation feature
provisions and read-only mounts Peritus's credential-free managed Python/Chromium runtime. The Bun
feature projects a credential-free Bun executable and any existing read-only package cache. The
module remains solely responsible for entry points, browser actions, routes, oracles, and result
interpretation.

The scoped-source `verify` operation rereads all included original bytes and executable bits
against the starting manifest; immutable artifacts alone are not substituted for that final live
comparison.
`module.source-build@1` is inspection-only from module code: the host-owned module market performs
the explicit build and installation, while the installed module may read its exact retained build
identity. It cannot trigger, widen, or retry its own build.
## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.
