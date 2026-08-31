# peritus-daemon

`peritus-daemon` is the single production composition owner for Peritus. It authenticates local
IPC peers, negotiates the A3 application protocol, serializes authority-bearing state mutations,
supervises bounded effect work, and coordinates deterministic startup, recovery, and shutdown.
Its independent A2 adapter exercises all 28 daemon cases through the public `peritusd` process,
including a real host PTY, a real C0 outbox crash after an external effect but before settlement,
both sides of the atomic B1 lease event/projection commit, and both sides of recoverable patch
application and the atomic D1 gate event/checkpoint commit.

The crate does not expose writable storage handles or reusable authority tokens. Embedders receive
configuration, lifecycle status, and a bounded authority client. Product-run configuration carries
default-off provider-failover consent into the runner. Provider switches are counted in durable run
progress and shown in live status without changing the A3 role-selection protocol.

The [G0 daemon guide](../../../docs/g0-daemon.md) documents strict configuration, startup and
recovery order, protected A3 IPC, durable service composition, outbox delivery, worker ownership,
shutdown, and the resource-aware verification commands. Operational procedures live in the
[recovery](../../../docs/g0-recovery-runbook.md) and
[shutdown](../../../docs/g0-shutdown-runbook.md) runbooks.

## Focused checks

From the repository root:

```sh
CARGO_BUILD_JOBS=2 cargo test --locked --package peritus-daemon
```
