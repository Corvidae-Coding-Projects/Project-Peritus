# peritus-daemon

`peritus-daemon` is the single production composition owner for Peritus. It authenticates local
IPC peers, negotiates the A3 application protocol, serializes authority-bearing state mutations,
supervises bounded effect work, and coordinates deterministic startup, recovery, and shutdown.
Its independent A2 adapter exercises all 28 daemon cases through the public `peritusd` process,
including a real host PTY, a real C0 outbox crash after an external effect but before settlement,
both sides of the atomic B1 lease event/projection commit, and both sides of recoverable patch
application and the atomic D1 gate event/checkpoint commit. Its qualification surface also proves
fresh startup replaces a corrupt derived projection and refuses a corrupt authoritative journal
before allocating authority.

The crate does not expose writable storage handles or reusable authority tokens. Embedders receive
configuration, lifecycle status, and a bounded authority client. Product-run configuration carries
default-off provider-failover consent into the runner. Provider switches are counted in durable run
progress and shown in live status without changing the A3 role-selection protocol.

Model discovery runs as bounded, connection-owned read tasks so provider latency does not stop
heartbeats, events, or other client requests. Each provider has its own discovery gate and readable
cache; repeated refreshes report an in-progress lookup instead of queuing more provider work.
Connection shutdown cancels and joins its outstanding discovery tasks. The A3 reader retains
partial headers and payloads across timer interrupts, with allocation limits checked before the
payload buffer is allocated.

Terminal detach and cancellation release live attachment slots and retain bounded exact retry
receipts separately (up to the configured attachment limit, oldest receipts evicted first).
Detaching leaves the process running and allows a new connection to attach. Admission reclaims
completed processes with no live attachments using the authoritative native result, even when
their output history has expired. Owner joins happen outside the registry mutex; attached output
still requires ordered delivery before retirement.
Delivery failures release only the failed attachment and notify its client without closing the
application connection or killing the process. Clients negotiating `app.terminal-failure` receive
an exact attachment-bound failure event; older clients receive an inert diagnostic. Other terminal
attachments and chat requests continue. Native transport failures still end the connection.

Existing interactive preview leases may use bounded pipes (including the raw Windows process
backend). Clients negotiating `app.terminal-pipes` can attach with an explicit pipe-mode reply,
send input, observe stdout/stderr, detach, and cancel. Pipe attachments reject resize operations.
This does not turn pipe children into PTYs or change process-tree containment; direct terminal
registration still requires a PTY plan.

Product-run records also retain candidate checkpoints, typed settlements, opaque continuation
state, remaining work, and interruption causes. Startup validates settled candidates against their
configured managed workspace, marks changed candidates stale, and automatically resumes
interrupted runs. Accept, commit, export, and discard remain separate durable user decisions, and
every mutating handoff action is revalidated against the exact candidate digest.

Interactive tool observations retain a bounded command/operation label before execution, then
update the same activity with the observed result. Command previews include exit status and
stdout/stderr; rejected calls and active background handles are not presented as completed success.
Recognizable credential fields are masked as accidental-disclosure defense, not as a complete
secret detector. Replacement bodies and terminal input are omitted from labels. No provider
reasoning or credential store is consulted. Existing activity persistence and wire formats remain
unchanged; terminal clients still sanitize control sequences before rendering.

The [G0 daemon guide](../../../docs/g0-daemon.md) documents strict configuration, startup and
recovery order, protected A3 IPC, durable service composition, outbox delivery, worker ownership,
shutdown, and the resource-aware verification commands. Operational procedures live in the
[recovery](../../../docs/g0-recovery-runbook.md) and
[shutdown](../../../docs/g0-shutdown-runbook.md) runbooks.

## Focused checks

The strict `[context.local]` configuration enables local working memory by default.
`peritusd context-inspect` reads the exact last published view without opening a writable owner.
The [local working-memory guide](../../../docs/local-working-memory.md) documents its arguments,
offline route admission, artifact retention, and fail-closed recovery boundaries.

From the repository root:

```sh
CARGO_BUILD_JOBS=2 cargo test --locked --package peritus-daemon
```
