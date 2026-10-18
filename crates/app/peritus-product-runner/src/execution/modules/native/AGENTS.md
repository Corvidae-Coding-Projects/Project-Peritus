# Native Rust plugin bridge

[native.rs](../native.rs) drives generic plugin actions; feature behavior belongs to the independently compiled module.

| File | Responsibility |
| --- | --- |
| [runtime.rs](runtime.rs) | Enabled-digest launch snapshot, existing native process host, live authority, framed service RPC and cancellation/cleanup. |
| [tools.rs](tools.rs) | Borrowed host service intersection and plugin-exported model functions; actual observation delivery. |
| [publication.rs](publication.rs) | Shared enabled-package/menu validation for final replies and durable continuing checkpoints. |

[Parent map](../AGENTS.md). Native code runs under the OS account; host-service mediation is not an OS sandbox.

A successful plugin function may return `request_complete: true` to yield immediately to its
coordinator after its durable checkpoint. Ordinary host web observations cannot trigger this.
The coordinator validates the saved output before advancing; this is not an approval bypass.

`Checkpoint` publishes conversation Markdown before `checkpoint_ack` continues the plugin.
Cancellation or new user input prevents acknowledgement. A crash between publication and
acknowledgement can replay that checkpoint; this contract favors retained work over losing it.
