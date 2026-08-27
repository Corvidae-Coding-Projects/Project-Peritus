# G0 recovery runbook

This runbook covers a local `peritusd` state root. Recovery preserves existing identities and
durable evidence. Do not delete the SQLite database, instance record, outbox rows, process records,
artifact objects, or migration backups merely to make startup proceed.

## First response

1. Stop any automated restart loop so one bounded diagnostic attempt can be observed clearly.
2. Preserve the exact configuration file and public approval-registry payload used by the failed
   start.
3. Record the stable `PERITUS-DAEMON-*` code, failed operation, process exit status, and whether a
   live `peritusd` still exists.
4. Check ownership and available space on the configured state root and every declared child root.
5. Restart once with the same binary, configuration, store ID, and paths. Recovery identities are
   deterministic; do not generate replacements.

The error's recovery classification determines the next action:

| Classification | Operator action |
|---|---|
| `CorrectRequest` | Correct the strict configuration or client input; do not mutate durable state. |
| `Retry` | Retry the same identity after bounded backoff. |
| `Reconcile` | Restart with the same inputs and allow native recovery to resolve recorded work. |
| `ReadOnly` | Keep the daemon diagnostic-only; preserve state for inspection. |
| `Operator` | Stop mutation attempts and investigate the named durable boundary. |

## Already-running or stale endpoint

`PERITUS-DAEMON-INSTANCE-001` means the state root has another proven live owner. Connect to that
owner or shut it down normally. Never remove its lock or socket while the recorded process identity
is live.

If the recorded process is absent, normal startup validates the record and removes only the exact
stale local endpoint before reacquiring ownership. A repeated already-running result after the
owner is demonstrably gone indicates an identity/probe problem; preserve the files and stop for
operator review.

## Migration failure

Migration is forward-only. The engine reconciles its durable operation record before planning new
work and resumes an incomplete backup or apply under the same deterministic operation identity.

- For `CorrectRequest`, fix path, version, compatibility, or free-space configuration.
- For `Retry` or `Reconcile`, retry the same daemon binary and state root.
- When the engine requests `RestoreBackup`, stop. Select the verified pre-migration backup named by
  the operation record and restore it using the migration tooling; do not copy arbitrary database
  files over a live state root.
- A registry digest or migration-history mismatch is `CorruptState`, not permission to rewrite the
  registry.

## Journal, projection, and application recovery

Startup opens C0 with the configured nonzero store identity and validates its integrity. A store-ID
mismatch or hash/position/checkpoint inconsistency must remain unavailable or read-only. Do not
change `store_id` to match an unexpected database and do not edit rows manually.

Rebuildable projections are checked against their source journal and replaced through the
projection store's compare-and-swap path. Application commands left pending or indeterminate are
resolved by their original domain command ID and request digest:

- definitely absent commands receive a stable rejection;
- committed commands recover their original event range;
- a digest conflict is corrupt state and stops mutation readiness.

No recovery path creates a new command identity for ambiguous work.

## Approval-registry drift

The configured file is a canonical B1 public credential-registry snapshot. On a fresh state root it
is installed exactly. On restart it must match the current durable bytes. A change must be the next
registry revision and carry a greater configured lineage generation.

If startup reports drift, restore the exact expected public snapshot or provision the exact legal
successor. Never place a private signing key in the file, lower the generation, skip revisions, or
replace one same-revision snapshot with different bytes.

## Outbox and effect recovery

Outbox rows are claimed with a positive authority-epoch fence and a bounded lease. After a crash,
the next owner decodes the original canonical claim and reconciles the destination's durable child
identity before acknowledging C0.

- A child already admitted under the exact deterministic identity is an idempotent retry.
- A stale fence cannot acknowledge the current claim.
- A destination mismatch, malformed payload, exhausted retry, or changed child binding enters
  read-only/operator recovery rather than skipping the row.
- Do not delete an outbox row to unblock the queue; it is part of the owning domain's durable truth.

The `qualify-outbox-stage` and `qualify-outbox-recover` administration commands deliberately create
and crash a deterministic test delivery. Run them only against a fresh disposable qualification
state root under the A2 supervisor, never against a state root being recovered for normal use.

## Processes, terminals, and workers

C2 reconciliation compares each process record with the native process birth identity. `Live`,
`Absent`, `Mismatched`, and `Indeterminate` remain distinct. An indeterminate process keeps the
daemon read-only and is reported as remaining work. Do not infer that a PID identifies the recorded
process after a restart.

Only a live daemon-owned PTY with known replay bounds may be reattached. If output continuity or
birth identity cannot be established, report the terminal as unavailable and retain the process
record for recovery. Worker tasks themselves are process-local; their durable domain inputs and
terminal observations determine restart behavior.

## Artifacts and telemetry

An artifact is authoritative only after exact size/digest finalization and catalog publication.
Incomplete upload files may be abandoned or quarantined by normal recovery; they must not be
inserted into the catalog manually. A catalog/object digest disagreement is corruption.

Local telemetry export is non-authoritative. Preserve its checkpoint and sequence-named files, but
telemetry failure must not be used to infer domain failure or success.

## Recovery completion

Recovery is complete only when the daemon reports `ready-read-write`, the singleton endpoint is
owned by the current process, no startup boundary remains unresolved, and the same command/outbox
identities resolve without duplicate effects. `ready-read-only` is useful diagnostic service, not
successful mutation recovery.
