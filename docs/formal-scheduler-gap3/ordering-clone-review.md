# GAP-03 reservation/history ordering and clone review

## Scope and snapshot

Read-only independent review against base `a41113d` of:

- `crates/orchestration/peritus-scheduler/src/state/ordering.rs` — `a6acd23d7c332ffbeef509c3f4d1f20cbc50a4f131d90a45faa1527d69a72d48`
- `crates/orchestration/peritus-scheduler/src/state/mutation.rs` — `4bee9c7f6947e2d7cc69208ad422d7ab86eca773233898752a4a6c6366a41321`
- `crates/orchestration/peritus-scheduler/src/state/mutation/insertion_slots.rs` — `c00638f032fde71dc2f29d22e6dd79a1389e35002a80515c8feb9d15c4b409e5`
- `crates/orchestration/peritus-scheduler/src/state/mutation/reservation_remove.rs` — `bbd7c24f5bb0befaa6ff928067afdf98dcb65b4ff5dca65de6ef10424b52cd23`
- `crates/orchestration/peritus-scheduler/src/state/clone_impl.rs` — `775483f8c71612174dc60d51a89012b0a0a4bd9c58749f421e628b5f1c42f9ad`
- `crates/orchestration/peritus-scheduler/src/state.rs` — `558887f32c03c6dede9598592c1c4f753184d9d2d50b10d98437e27a87c75e5b`
- `crates/orchestration/peritus-scheduler/src/binding.rs` — `e1ec9916fb86ada0eb11622a2d2f1402a4a842b281c1a464916bee748cf19f4d`
- `crates/orchestration/peritus-scheduler/tests/canonical_lookup.rs` — `204b07402913305e6151a3de59d87549f1d5891a5deba449d5822f2b951f1fa2`

No source was edited and no command in the build lane was run by this reviewer.

## Verdict

PASS for the bounded increment. No correctness, runtime-behavior, wire-format, or diagnostic regression was found in scope.

The reservation binary slot loop, historical-dispatch linear slot loop, reservation insertion, historical identity insertion, reservation removal, state clone, genesis initialization, and state/binding accessors retain their prior executable statements and branch order. The slot algorithms were extracted and instrumented with ghost/proof code; proof annotations erase. No codec/canonical/digest/error text was changed.

The strict ordering predicates quantify every earlier/later pair over the exact production identity projections. Reservation insertion requires strict prior ordering, exact dispatch-ID freshness, and the slot partition. Historical insertion likewise requires absence before asserting strict order. Public mutation functions have no new executable or hidden preconditions: preservation is a conditional postcondition guarded by old ordering and freshness. Removal correctly needs old ordering but no freshness. Empty, front, middle, and end slots satisfy the stated partitions.

All six collection quantifiers in `clone_equivalent` and `reservation_clone_equivalent` are now parenthesized. Later collection lengths, histories, ordinals, commands, and terminal evidence therefore remain independent constraints when an earlier collection is empty. `clone_preserves_authoritative_fields` explicitly projects those facts; binding, worker, work, reservation, and terminal clone relations cover every stored field. New binding/state accessors return the same values as before with exact postconditions. Genesis creates empty ordered collections and retains the prior runtime initialization.

The extended canonical lookup test varies every first-difference byte position, inserts workers/work/dispatches out of order, compares three binary lookups to linear lookup, asserts all four collections remain strictly ordered, releases every reservation, confirms dispatch history does not change, and confirms replay equality.

## Evidence reviewed

- `target/gap3-evidence/30-restored-scheduler-verus.log`: strict `415 verified, 0 errors`.
- `target/gap3-evidence/28-clone-grouping-mutation.log`: restored old grouping is rejected at exact work and reservation length projections (`414 verified, 1 errors`; two reported failed postconditions).
- `target/gap3-evidence/33-scheduler-tests.log`: 35 scheduler tests pass, including the extended canonical lookup/release test.
- `target/gap3-evidence/34-scheduler-clippy.log`: strict scheduler Clippy completes successfully.

## Remaining proof gaps outside this increment

- `reserve_selected_at` and higher release/reducer wrappers do not yet publish a combined `spec_collections_ordered` preservation contract. The leaf mutations provide the required conditional facts, but end-to-end reducer composition remains open.
- `spec_collections_ordered` is a separate property rather than part of `spec_reservation_reducer_ready`; decoded `from_wire` state and validation are not yet proven to establish it here.
- Genesis proves the ordering-relevant empty sequences and dispatch ordinal, but its postcondition does not yet project every newly exposed authoritative scalar (`sequence`, `last_event_id`, zero digest, enqueue ordinal, initial used command, and absent terminal).

These are scope boundaries, not counterexamples to the reviewed leaf contracts.

## Error-trait metadata probe note

A small next compiler probe may declare a metadata-only `external_trait_specification` for `core::error::Error`, mirroring only its `Debug + Display` supertraits and its `ExternalTraitSpecificationFor` association. vstd already contains metadata-only bindings for `core::fmt::Debug` and `core::fmt::Display`. This could populate the trait path missing in log 32 without a trusted body or method postcondition. It may still fail because `Error` contains self-referential dynamic-object methods; no scheduler change should depend on it until the disposable probe succeeds.

This is an agent-produced review. It is not a human approval record.
