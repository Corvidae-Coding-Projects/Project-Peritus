# peritus-daemon

`peritus-daemon` is the single production composition owner for Peritus. It authenticates local
IPC peers, negotiates the A3 application protocol, serializes authority-bearing state mutations,
supervises bounded effect work, and coordinates deterministic startup, recovery, and shutdown.
Its independent A2 adapter exercises all 28 daemon cases through the public `peritusd` process,
including a real host PTY and a real C0 outbox crash after an external effect but before settlement.

The crate does not expose writable storage handles or reusable authority tokens. Embedders receive
configuration, lifecycle status, and a bounded authority client.

The [G0 daemon guide](../../../docs/g0-daemon.md) documents strict configuration, startup and
recovery order, protected A3 IPC, durable service composition, outbox delivery, worker ownership,
shutdown, and the resource-aware verification commands. Operational procedures live in the
[recovery](../../../docs/g0-recovery-runbook.md) and
[shutdown](../../../docs/g0-shutdown-runbook.md) runbooks.
