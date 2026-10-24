# Native Rust plugin bridge

[native.rs](../native.rs) drives generic plugin actions; feature behavior belongs to the independently compiled module.

| File | Responsibility |
| --- | --- |
| [runtime.rs](runtime.rs) | Enabled-digest launch snapshot, existing native process host, live authority, framed service RPC, exact writer/reviewer route metadata, and cancellation/cleanup. |
| [scoped_source.rs](scoped_source.rs) | Explicit current-workspace capture, immutable content-addressed manifests, revocable handles, mandatory Git/Peritus metadata exclusions, bounded range evidence, reuse and corruption fail-closure. |
| [owned_jobs.rs](owned_jobs.rs) | Durable asynchronous Linux job broker, live authority monitoring, contained canary proof adapter, logs, cancellation, receipts, and owner-loss recovery. |
| [dependency_preparation.rs](dependency_preparation.rs) | Credential-free public-HTTPS preparation with DNS/IP checks, bounded redirects and bytes, and exact SHA-256 admission. |
| [tools.rs](tools.rs) | Borrowed host service intersection and plugin-exported model functions; actual observation delivery. |
| [publication.rs](publication.rs) | Shared enabled-package/menu validation for final replies and durable continuing checkpoints. |

[Parent map](../AGENTS.md). Native code runs under the OS account; host-service mediation is not an OS sandbox.

A successful plugin function may return `request_complete: true` to yield immediately to its
coordinator after its durable checkpoint. Ordinary host web observations cannot trigger this.
The coordinator validates the saved output before advancing; this is not an approval bypass.

`Checkpoint` publishes conversation Markdown before `checkpoint_ack` continues the plugin.
Cancellation or new user input prevents acknowledgement. A crash between publication and
acknowledgement can replay that checkpoint; this contract favors retained work over losing it.

Every module call receives the additive versioned host capability profile. `HostService` requests
are returned through `host_result`; unsupported source, job, or source-build services are explicit
outcomes and are never treated as permission to fall back to ambient effects.

On configured Linux hosts, `module.owned-jobs@1` provides submit, poll, paged log, and cancel. The
proof adapter uses the existing native private-filesystem backend, cleared environment, deny-all
network and secrets, bounded resources, hidden host canaries, and host-authored receipts. The
separate dependency-preparation adapter permits only exact digest-pinned public HTTPS retrieval
without credentials, then hands only the content-addressed artifact identity to later work.
`module.source-build@1` is inspection-only from module code: the host-owned module market performs
the explicit build and installation, while the installed module may read its exact retained build
identity. It cannot trigger, widen, or retry its own build.
