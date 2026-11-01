# Stage 6 authority model contract

`stage6::authority` calls the public `peritus-budget` reducer and the public
`peritus-policy` authority-time primitives. Generated cases use only synthetic
identifiers and in-memory values. A `ReservationReference` in this harness is a
logical input; it does not attest that an external operation never activated.

The budget reference model independently sums token consumption, live held
capacity and attempt charges with `u128` arithmetic. It generates begin,
activate, cancel, exact settlement, seal and close histories, including exact
replays, conflicting reservation reuse, oversized begins, unknown reservation
operations, and attempts after sealing. After each step it compares the public
account and reservation projections, confirms accepted receipt kind, checks
rejected-command atomicity, and calls the reducer invariant validator. The
targeted replay prefix makes the `stage6-authority.patch` mutation observable
without relying on accidental duplicate generation.

The policy model treats an authority clock as a monotonic floor within one
epoch, validity windows as half-open integer intervals, use bounds as
`Option<u64>` with `None` representing unlimited, and time addition as `u128`
addition checked against `u64::MAX`. It checks epoch mismatch, regression,
constructor validity, containment, intersection, zero-use rejection and exact
overflow behavior.

`stage6::router` uses the existing Stage 3 fixture to commit synthetic kernel,
capability, budget and current-epoch evidence into a disposable SQLite store.
It prepares one C4 call and dispatches it through the actual `ToolRouter` and
a counting dispatcher. Generated cases first try stale authority before the
valid effect; that denial must leave the dispatcher untouched and let the
later exact call start once. After the terminal non-idempotent outcome, a generated
sequence retries the same call with current or stale time, wrong epoch,
prepared digest, generation or session, and with a changed idempotency key.
Every retry must leave the effect count at one. Exact current replay reports
the prior non-idempotent terminal; stale authority returns Authorization;
changed call identity returns ReplayConflict. The router calibration patch
removes replay's current-authority validation and must make the retained stale
deadline example fail.

These targets do not model the full policy definition/approval authority
matrix, multi-account budget delegation, cumulative usage observations, or
process-crash persistence. The separate `stage6::kernel` target models the
session/run/attempt/turn subset of the reducer; its remaining boundaries are
listed in `stage6-policy-gaps.md`. `stage6-router-fixture.patch`
adds a case-only accessor to the existing disposable authority fixture and
mounts the router target under it; it changes no production API. The two
calibration patches are disposable mutations only and must never enter
production source.

Constructive prefixes force one exact begin replay, one active settlement,
and one held cancellation/replay across arbitrary suffixes. A separate
window generator constructs overlapping valid intervals so intersection
success does not depend on rare random endpoints. Outcome metrics distinguish
successful exact settlement and held cancellation.
