# peritus-scheduler

## Folder guide

Durable deterministic bounded resource scheduling and worker ownership.

Active root-workspace package; declarations and targets are in [Cargo.toml](Cargo.toml).

Use [AGENTS.md](AGENTS.md) for file-level responsibilities, child modules, parent context, and nearby implementation owners.

### Local files

| File | Role |
| --- | --- |
| [Cargo.toml](Cargo.toml) | Cargo package/dependency/target declarations for `peritus-scheduler`. |
| [README.md](README.md) | Human guide and operating/background notes for this folder; AGENTS.md provides the local navigation map. |

### Subfolders

| Folder | Role / next waypoint |
| --- | --- |
| [src/](src/AGENTS.md) | Durable deterministic bounded resource scheduling for Peritus. |

`peritus-scheduler` is the D3 run-scoped scheduler aggregate. It owns bounded work admission,
dependency readiness, deterministic fair selection, worker/resource reservations, retry and loss
classification, pause/drain/cancellation, truthful finalization, replay, and inert runtime
directives. It never executes work, grants authority, mutates a workspace, or interprets payloads.

Families 70, 71, and 72 carry command, event, and state frames. New aggregates use schema 2;
historical schema-1 aggregates keep their exact bytes, replay decisions, and continuations.
Accepted
transitions commit one event and the complete successor checkpoint under C0 namespace `0xD301`.
Dispatch and cancellation effects are emitted only from already-committed state and carry stable
idempotency identities for safe restart/redelivery.

Resource and scheduling decisions are pure and time-independent: configured capacities, queue
ordinals, priority, and bounded bypass counters completely determine every reservation.
Worker loss, cancellation, and retry never infer success from an absent or late observation.

Schema 2 reserves queue capacity for waiting work and for active work that still has another
attempt available. Recovery can therefore return active work without exceeding the queue bound.
Schema 1 retains historical admission and validates its reachable waiting-plus-active bound.
The aggregate's semantics never change in place. Use `SchedulerCommand::from_state` for
continuations and `encode_scheduler_*`/`decode_scheduler_*` helpers for version-aware frames.
The public frame wrappers describe schema 2 and reject legacy values during encoding.

`SchedulerSession` retains at most one fully verified replay for the serialized daemon owner.
Warm commands use the same pure reducer and canonical checkpoints, then retain a successor only
after a guarded C0 commit receipt. A run switch, journal reopen, another append, external commit,
or failed append requires checked cold replay. E0's historical pause/resume and exact retry readers
continue to use the complete retained event history and preserve the predecessor's semantics.

## Behavioral work

Use the [project real-testing policy](../../../README.md#real-testing) and the actual interfaces owned by this crate. The [local map](AGENTS.md) identifies entrypoints and implementation owners.
