# Native Rust plugin bridge

[native.rs](../native.rs) drives generic plugin actions; feature behavior belongs to the independently compiled module.

| File | Responsibility |
| --- | --- |
| [runtime.rs](runtime.rs) | Enabled-digest launch snapshot, existing native process host, live authority, framed service RPC and cancellation/cleanup. |
| [tools.rs](tools.rs) | Borrowed host service intersection and plugin-exported model functions; actual observation delivery. |

[Parent map](../AGENTS.md). Native code runs under the OS account; host-service mediation is not an OS sandbox.
