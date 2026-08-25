# peritus-scheduler

`peritus-scheduler` is the D3 run-scoped scheduler aggregate. It owns bounded work admission,
dependency readiness, deterministic fair selection, worker/resource reservations, retry and loss
classification, pause/drain/cancellation, truthful finalization, replay, and inert runtime
directives. It never executes work, grants authority, mutates a workspace, or interprets payloads.

Families 70, 71, and 72 are the immutable schema-v1 command, event, and state frames. Accepted
transitions commit one event and the complete successor checkpoint under C0 namespace `0xD301`.
Dispatch and cancellation effects are emitted only from already-committed state and carry stable
idempotency identities for safe restart/redelivery.

Resource and scheduling decisions are pure and time-independent: configured capacities, queue
ordinals, priority, and bounded bypass counters completely determine every reservation.
Worker loss, cancellation, and retry never infer success from an absent or late observation.
