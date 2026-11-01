# Stage 6 kernel lifecycle contract

`stage6::kernel` invokes `peritus_kernel::KernelAggregate::open` and `reduce`
through public APIs. The only shared fixture is an immutable synthetic
`AcceptanceContract` and nominal identifiers from Stage 3. The expected
session/run/attempt/turn phases are a separate small state table; the target
does not call production phase predicates or reducer helpers to decide whether
a command should be accepted.

Generated histories mix session pause/resume/close, run admission/pause/resume
and four nonaccepting terminal outcomes, attempt admission/submission/failure/
exhaustion, and turn start/completion/failure/cancellation. Constructive
prefixes reach a blocked session pause while a pending run exists and a deeper
run/attempt/turn/reviewing/paused sequence. Four deliberately wrong envelope
variants exercise revision, causal head, command ID, and event ID preflight.
The seven fixed examples collectively reach all 18 modeled commands as accepted
transitions, including fail/cancel turn, exhaust attempt, and reject run;
the randomized search need not discover these narrow paths by chance.
Every accepted command must emit exactly one kind/subject/identity/sequence
bound event; every rejection must return the exact unchanged aggregate.
After each step the target compares public session, run, attempt and turn
projections with its model and checks the aggregate invariant auditor.

The mutation in `calibration/stage6-kernel.patch` removes the live-run guard
from `PauseSession`. The first fixed example and the generated blocked-pause
prefix must detect that change. This mutation belongs only in a disposable
calibration worktree.

The target does not cover action authorization/dispatch, review, waiver,
acceptance settlement, multiple run or attempt identities, or crash durability.
`stage6::router` separately covers one committed tool dispatch/replay boundary.
The full `PolicyDefinition` authority matrix and quality/security/release
criteria are separate Stage 6 responsibilities; this target supplies only the
exact B0 lifecycle evidence to their consumers.
