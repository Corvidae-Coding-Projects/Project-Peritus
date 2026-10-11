# peritus-app-client

Shared local application-protocol transport for Peritus user interfaces. The daemon remains the authority for conversations, approvals, execution, and durable recovery.

`Client` negotiates a local IPC connection, enforces bounded PRTS frames, and matches responses against the negotiated context and both request identifiers. Requests require exclusive mutable access. Event subscriptions use separate connections.

Retain `RequestIdentity` and the payload's operation identity before submission. A timeout, failed exchange, or cancelled future invalidates the connection. Reconnect and reconcile the original durable receipt before retrying an uncertain mutation; a transport failure does not prove that no effect occurred.

The request deadline includes writing and waiting for the response. Event reads have no request deadline; their consumer owns idle detection and reconnection. Cancelling an event read requires replacing that connection because the read may have consumed a partial frame.

Modules separate negotiation (`connection`), frame transport (`frame`), request matching (`request`), subscription/control traffic (`events`), identities (`identity`), and typed failures (`error`).

## Focused checks

From the repository root:

```text
cargo fmt -p peritus-app-client -- --check
cargo test --locked -p peritus-app-client --jobs 2
cargo clippy --locked -p peritus-app-client --all-targets --jobs 2 -- -D warnings
```

These checks cover this package only; they do not establish WebUI completion or CLI integration.
