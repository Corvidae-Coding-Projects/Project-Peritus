# peritus-budget

`peritus-budget` owns Peritus's pure, verified accounting decisions for model tokens, provider
cost, accountable active-effect time, attempts, and retries. One ledger contains a root budget and
its complete child tree so reservation changes and inclusive ancestor accounting are atomic.

## Invariants

- Accounted consumption never decreases.
- For every account and dimension, consumed, operation-reserved, and delegated-remaining capacity
  together never exceed the immutable limit.
- A child receives only currently available parent capacity. Descendant consumption is moved from
  delegated remaining capacity into consumed capacity at every ancestor in the same transition.
- Cumulative observations charge only a nonnegative high-water delta. Provider corrections cannot
  refund consumption.
- Only a held, never-activated reservation may be cancelled without usage evidence. Ambiguous
  active work consumes its remaining ceiling.
- A retry is derived from immutable exact-action history, uses a fresh reservation identity, waits
  for the prior attempt to reach a terminal state, and charges both an attempt and retry before
  work.
- Equal cumulative observations replay only with the same retained evidence binding; a different
  evidence digest cannot masquerade as an idempotent observation.
- Arithmetic never wraps or saturates. Rejected transitions construct no next state.

This crate is deliberately effect-free. Its transitions and receipts are logical accounting facts,
not dispatch permits or evidence that a journal commit or external effect succeeded. It does not
serialize privileged state; later protocol code must convert checked commands and replay accepted
transitions.

`ReservationReference` is intentionally a freely constructible correlation value. It can bind a
logical command to an existing reservation tombstone, but it cannot prove the external negative
fact that an operation never activated. `CancelHeld`, its receipt, and its transition are therefore
non-authorizing logical plans. `REF-C0-B1-COMMIT-ONCE` requires C0 to match the committed begin
lineage and its own non-forgeable authoritative target or journal observation before committing a
cancellation. Mismatched, stale, duplicate-with-different-evidence, active, and indeterminate
claims do not release capacity in this reducer; B1 does not overclaim that this establishes
external truth.

## Dependency policy

This verification-class `V` crate depends only on `peritus-types` and the pinned `vstd`. A2 test
support is a development dependency only.
