# G0 shutdown runbook

`peritusd` accepts shutdown from an authenticated A3 request or an operating-system termination
signal. Both paths use the same bounded cleanup order. A client request retains its request and
correlation identities in A3 progress/completion; a signal does not invent protocol identity.

## Normal shutdown

Prefer an authenticated A3 shutdown request when a client is available. Otherwise send the normal
service-manager termination signal. Do not start a second daemon against the same state root while
shutdown is in progress.

The runtime performs these stages:

1. close new worker admission and tell ordinary connections to drain;
2. keep the authenticated shutdown requester open as the bounded reporting connection;
3. stop the outbox pump without acknowledging unsettled claims;
4. put the authority owner into draining readiness;
5. request cooperative worker cancellation, then bounded joining;
6. reconcile native C2 process records;
7. checkpoint the configured telemetry exporter;
8. stop and join the sole authority owner;
9. after streaming six stage observations, send the exact clean or unclean completion;
10. join the reporting connection and local endpoint, then release process-owned singleton
    resources.

An individual cleanup failure is retained while later stages continue. This prevents an early
connection or worker failure from skipping process, telemetry, authority, or lock cleanup.

Use `peritus --endpoint <address> shutdown --wait` when an authenticated CLI is available. The
command keeps the connection open, prints bounded progress, and returns only after it receives the
correlated completion. A client that disconnects does not prevent daemon cleanup; it only gives up
the completion report.

## Interpreting completion

Clean completion means every owned registry reports zero remaining work. An unclean result includes
bounded fixed-vocabulary summaries for any remaining requests, subscriptions, artifact transfers,
prompts, terminal attachments, outbox deliveries, workers, processes, telemetry batches, or
indeterminate effects.

`peritusd` exits zero only for clean completion. It exits nonzero with
`PERITUS-DAEMON-SHUTDOWN-001` or another stable category when cleanup is incomplete. Treat that as a
restart-reconciliation requirement, not as permission to erase retained state.

## Timeout or stuck work

The configured `shutdown_millis` bounds each owned join boundary. On timeout the daemon aborts only
the process-local task it owns, records an unclean failure, and continues later cleanup. Durable
commands, outbox rows, process records, and artifacts remain for the next startup.

If shutdown exceeds the service manager's outer deadline:

1. capture the last reported stage and remaining-work categories;
2. allow the configured daemon deadline to expire when possible;
3. if an external kill is unavoidable, kill only the exact daemon process;
4. preserve the state root and restart with the same binary/configuration;
5. verify startup reconciliation before allowing new mutation work.

Do not recursively kill child PIDs by number alone. C2 process identity includes native birth facts,
and a reused PID is not evidence that a process belongs to this daemon.

## Forced-kill restart

After `SIGKILL`, power loss, or host termination there is no shutdown completion record. Restart is
the recovery procedure. The next daemon must reacquire the singleton, allocate a new authority
epoch, reconcile application commands and process records, and retry durable outbox claims under
their original identities.

Expected safe outcomes include exact command replay, idempotent child admission, expiry of old
claim leases, and explicit indeterminate/read-only status. Duplicate effects, a silently discarded
outbox row, automatic approval, or a clean-shutdown claim are not acceptable recovery outcomes.

## Verification after shutdown

For a clean operational handoff, confirm:

- the daemon process has exited with status zero;
- the local socket or pipe is no longer served by the old process;
- no worker or process is reported as remaining;
- telemetry shutdown reported no unacknowledged bounded batch;
- a subsequent start with the same state root succeeds and replays the last durable command result;
- a second simultaneous start is still rejected without replacing the live endpoint.
