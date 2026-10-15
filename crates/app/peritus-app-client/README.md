# peritus-app-client

## Folder guide

Shared local application-protocol client for Peritus interfaces.

Active root-workspace package; declarations and targets are in [Cargo.toml](Cargo.toml).

Use [AGENTS.md](AGENTS.md) for file-level responsibilities, child modules, parent context, and nearby implementation owners.

### Local files

| File | Role |
| --- | --- |
| [Cargo.toml](Cargo.toml) | Cargo package/dependency/target declarations for `peritus-app-client`. |
| [README.md](README.md) | Human guide and operating/background notes for this folder; AGENTS.md provides the local navigation map. |

### Subfolders

| Folder | Role / next waypoint |
| --- | --- |
| [src/](src/AGENTS.md) | Shared local transport contracts for Peritus user interfaces. |

Shared local application-protocol transport for Peritus user interfaces. The daemon remains the authority for conversations, approvals, execution, and durable recovery.

`Client` negotiates a local IPC connection, enforces bounded PRTS frames, and matches responses against the negotiated context and both request identifiers. Requests require exclusive mutable access. Event subscriptions use separate connections.

Retain `RequestIdentity` and the payload's operation identity before submission. A timeout, failed exchange, or cancelled future invalidates the connection. Reconnect and reconcile the original durable receipt before retrying an uncertain mutation; a transport failure does not prove that no effect occurred.

The request deadline includes writing and waiting for the response. Event reads have no request deadline; their consumer owns idle detection and reconnection. Cancelling an event read requires replacing that connection because the read may have consumed a partial frame.

Modules separate negotiation (`connection`), frame transport (`frame`), request matching (`request`), subscription/control traffic (`events`), identities (`identity`), and typed failures (`error`).

## Behavioral work

Use the [project real-testing policy](../../../README.md#real-testing) and the actual interfaces owned by this crate. The [local map](AGENTS.md) identifies entrypoints and implementation owners.
