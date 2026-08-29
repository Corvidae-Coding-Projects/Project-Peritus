# G0 daemon and application composition

G0 is the production application root for Peritus. The `peritusd` process owns one protected local
state root, one writable C0 journal connection, the A3 local application endpoint, configured
providers and tools, bounded workers, durable outbox delivery, telemetry export, and orderly
shutdown. It composes the existing A through F subsystems; it does not replace their reducers or
manufacture authority on their behalf.

## Authority and ownership

`AuthorityOwner` is the only task allowed to mutate the journal. Every application session,
idempotency record, domain command, artifact publication, prompt settlement, and outbox claim is
serialized through its bounded channel. Effect workers receive typed claims or immutable inputs,
never a journal handle or reusable authority token.

The ordering rule is commit before effect:

1. the owning domain commits a directive and C0 outbox row;
2. the daemon claims the exact row with a positive authority-epoch fence;
3. the destination adapter durably and idempotently admits the child operation;
4. the owner acknowledges the domain directive and exact C0 claim;
5. the child later publishes its typed observation through its own reducer.

A crash after child admission but before acknowledgement retries the same deterministic child
identity. A stale claim fence, changed child binding, or ambiguous durable result is reported as
recovery work; it is never converted into success.

## Configuration

Normal operation uses one production command:

```text
peritusd serve --config /absolute/path/peritus.toml
```

G0 also exposes three bounded administration entry points used by A2 qualification:

```text
peritusd qualify-pty
peritusd qualify-outbox-stage --config /absolute/path/isolated-qualification.toml
peritusd qualify-outbox-recover --config /absolute/path/isolated-qualification.toml
```

`qualify-pty` launches and reaps a real host PTY child and reports the directly observed combined
stream ordering, offsets, exit fence, and buffering bound. The outbox pair writes one deterministic
identity-bearing filesystem effect, publishes a flushed post-effect/pre-ack checkpoint, is killed
by the qualification supervisor, then reclaims and acknowledges the exact live C0 fence without a
duplicate effect. The outbox commands mutate their configured journal and are only for a fresh,
disposable qualification state root; they are not repair commands for a live daemon. These narrow
administration commands do not constitute the G1 user CLI.

Configuration is strict version-one TOML. Unknown fields, relative or overlapping protected
roots, zero or malformed identities, duplicate projects/workspaces/tools/providers, plaintext
credential material, and limits outside the compiled ceilings are rejected before readiness.

A minimal isolated configuration has this shape:

```toml
version = 1
store_id = "11111111111111111111111111111111"

[paths]
state_root = "/srv/peritus/state"
artifact_root = "/srv/peritus/state/artifacts"
evidence_root = "/srv/peritus/state/evidence"
workspace_root = "/srv/peritus/state/workspaces"
process_root = "/srv/peritus/state/processes"
transaction_root = "/srv/peritus/state/transactions"
backup_root = "/srv/peritus/state/backups"

[approval_registry]
payload_file = "/etc/peritus/approval-registry.bin"
generation = 1

[human]
actor_id = "22222222222222222222222222222222"

[product]
automatic_provider_failover = false

[telemetry]
mode = "disabled"
```

The approval-registry payload is the canonical B1 public credential snapshot. It contains public
verification material only. The human signing key stays in the user's external signing system and
is never placed in daemon configuration or C0.

Projects contain stable project IDs and their workspace IDs. Each workspace entry names an
absolute file containing a canonical C1 registration envelope. The tool policy is an explicit
allowlist over the compiled C4 catalog; an empty allowlist exposes no tools. Provider routes bind a
stable profile ID and revision to one exact adapter. Direct HTTP routes use opaque C3 credential
references. Account-backed OpenAI and Anthropic routes use the official `codex` and `claude`
executables as credential-owning routers, either discovered on `PATH` or selected by an absolute
configured path.

The optional `[product]` table carries user-selected run behavior rather than provider secrets or
live health claims. `automatic_provider_failover` defaults to `false` for older configuration. When
enabled, the daemon supplies the configured provider inventory to the product runner as a
deterministic fallback chain; the runner still applies capability and failure-category checks.

Telemetry is either `disabled` or a bounded `local-file` spool beneath a protected absolute path.
There is no implicit network exporter.

## Startup and recovery

Startup executes a closed monotonic sequence:

```text
Validate -> Lock -> Migrate -> Journal -> Artifacts -> Evidence -> Projections
  -> AuthorityEpoch -> DomainRecovery -> EffectRecovery -> AppRecovery
  -> Outbox -> IPC -> Ready
```

The daemon first creates and protects its declared roots, then acquires the state-root singleton
lock. It opens or migrates the shared database, verifies store identity, opens artifact/evidence
stores, rebuilds or validates projections, and installs the configured public approval registry.
An exact restart is idempotent; a registry update must be the exact next revision with an increased
lineage generation. Same-revision drift and skipped or regressing lineage are rejected.

Each successful start allocates a new durable authority epoch. Prior claim leases, approval time
observations, and process observations cannot be reused as current authority. Registered
workspaces, the F0 production pointer, C2 processes, pending application commands, and durable
outbox rows are reconciled before mutation readiness. Safe diagnostic failures enter explicit
read-only readiness. Failures that cannot support trustworthy diagnostics leave the daemon
unavailable.

The startup plan has named failpoints before and after every meaningful stage. Restart tests prove
that a killed process observes the exact committed prefix and never infers completion from a file
or process merely existing.

## Local application protocol

G0 exposes A3 only over a local operating-system endpoint. Unix platforms use a protected
Unix-domain socket. Windows uses a local named pipe with an owner-restricted security descriptor.
There is no TCP listener or remote bind option.

The endpoint authenticates the operating-system peer before negotiation and resolves it through
the durable one-to-one local-principal binding. A successful hello establishes or resumes a
durable session. Every later frame must echo the exact negotiated protocol ID, version, session,
and actor. The frame reader validates the fixed PRTS header before allocating the bounded payload.

The request surface provides:

- idempotent B3 command submission with durable pending/final application records and exact C0
  committed ranges;
- resumable, filtered, at-least-once event subscriptions with source cursors, cumulative
  acknowledgement, bounded delivery windows, gaps, pause/resume, and backpressure;
- streaming artifact download and upload with exact identity, offsets, byte counts, size, digest,
  ownership, cancellation, and durable catalog publication;
- prompt registration and responses bound to the complete actor/session/revision/cancellation
  correlation, including strict client-signed B1 approval authentication;
- C2-owned terminal attach, ordered PTY output, bounded input and resize, detach, cancellation,
  exit, and restart-safe reattachment classification;
- read-only daemon status and authenticated graceful-shutdown requests.

Client-supplied protocol values remain input, not authority. Signed approval verifies an existing
B1 decision; it does not bypass B1 currentness, one-use consumption, or the target domain commit.
Terminal bytes, model output, repository content, diagnostics, and prompt text are bounded inert
data.

## Components and workers

Startup constructs one immutable provider/tool inventory. Profile identity plus revision selects a
single provider adapter. Tool descriptors are drawn from the compiled filesystem, Git, quality,
and shell catalogs and are exposed only when named by configuration. The daemon does not discover
native plugins.

`WorkerSupervisor` owns each spawned task, cancellation signal, join handle, and terminal
observation under configured capacity. Duplicate dispatch identity is rejected. Shutdown first
requests cooperative cancellation, then performs bounded joins and reports any forced abort or
unreaped observation as remaining work. Task counters are diagnostic summaries; durable domain
state remains the authority.

The outbox router is a closed destination registry. Unknown destinations and malformed claims are
not sent to a generic callback. Each registered adapter decodes the owning domain's canonical claim,
reconciles the destination's durable identity, and uses the destination's native transition API.

## Shutdown

An authenticated A3 request or operating-system signal begins the same ordered shutdown:

```text
close admission -> join connections -> stop outbox -> stop workers
  -> reconcile processes -> stop authority -> release process-owned resources
```

Cleanup continues after an individual stage failure so later resources are not leaked. The final
result is clean only when every owned registry reports no remaining requests, subscriptions,
transfers, prompts, terminal bridges, outbox deliveries, workers, processes, or telemetry work.
`peritusd` exits nonzero for an unclean result and prints only bounded stable categories.

## Verification

Focused development runs one heavy workload at a time:

```text
CARGO_BUILD_JOBS=1 cargo test --package peritus-daemon --all-targets --all-features --locked
CARGO_BUILD_JOBS=1 cargo test --package peritus-conformance --all-targets --all-features --locked
CARGO_BUILD_JOBS=1 cargo clippy --package peritus-daemon --all-targets --all-features --locked -- -D warnings
CARGO_BUILD_JOBS=1 RUSTDOCFLAGS='-D warnings' cargo doc --package peritus-daemon --all-features --no-deps --locked
CARGO_BUILD_JOBS=1 cargo verus verify --package peritus-daemon --all-features --locked --check-toolchain --fwd-verus-args-to roots -- --no-cheating --rlimit 20
```

The independent A2 daemon contract has 28 cases covering negotiation, authentication, command
idempotency, subscriptions, artifacts, prompts, terminals, read-only admission, singleton
ownership, startup/outbox crash windows, shutdown/restart, malformed frames, bounds, and
non-authority behavior. All 28 cases execute through the public `peritusd` subprocess boundary;
none is represented by an unavailable-case waiver. The final merge authority remains
`CARGO_BUILD_JOBS=1 just gate-a` plus the required Ubuntu, macOS, Windows, and Foundation hosted
checks.

Operational incident procedures are separated into the
[recovery runbook](g0-recovery-runbook.md) and [shutdown runbook](g0-shutdown-runbook.md).

G0 is application composition, not the eventual user-facing CLI or TUI. Those remain G1 and G2.
