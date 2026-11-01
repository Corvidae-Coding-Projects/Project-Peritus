# State campaign strengthening

This follow-up inspects generator support and observable assertions, rather than treating a passing
case count as whole-program coverage. Four targets receive new operation families:

| Target | Gap in the earlier suite | Added search/oracle |
| --- | --- | --- |
| `context` | Graph topology stayed fixed after construction; replacement only changed text/status. | Edge replacement after reads, edge deletion/readdition, multi-node cycles, exact retained adjacency, and both incremental and rebuilt graphs checked against independent BFS membership at allocation boundaries. |
| `memory` | Independent random scopes/filters frequently excluded every candidate; individual lifecycle operations did not guarantee a useful query afterward. | Correlated competing eligible records, varied ranking/budgets, warm query → quarantine → query → release → query → same-size replacement → deletion → import → query. Counters distinguish queries with eligible and competing candidates. |
| `review` | Restore duplicated identical rows only; random reviews rarely revisited a resolved identity. | Unlike legacy rows spanning observation cycles and lifecycle states, rotated/reversed/repeated input, earliest-admission conservation, and explicit omission/fix/confirm/restore/reappearance chains. Equal-cycle/equal-state ambiguous payload ties deliberately share payloads. |
| `modules` | Random absolute stage IDs produced many rejected advances; page tests did not consume all continuation tokens; corruption tests reopened only a one-stage job. | Constructive multi-stage completion and reopen histories, complete Unicode page walks with exact concatenation/progress, and live same-size/truncated/missing corruption at every completed stage while the owning job stays open. |

`modules` found a real read-path gap: `Job::inspect` validated only the last output while exposing
metadata for every completed stage. A damaged predecessor therefore appeared in a successful
completed snapshot. The fix verifies predecessors before reading the final output; inspection
continues to work without taking the writer's ownership lock. The focused public consumer is
`crates/orchestration/peritus-modules/examples/inspect_predecessor_integrity.rs`.

```sh
cargo run --offline --locked -p peritus-modules --example inspect_predecessor_integrity
python3 discovery/proptest-praxis/campaign.py build --output /tmp/praxis-state-build-new
python3 discovery/proptest-praxis/campaign.py search --target context --cases 512 --max-steps 64 --seed 19031 --output /tmp/praxis-context-new
```

Use the same search command with `memory`, `review`, and `modules`. Each output directory must be
new. Actual run receipts/failure witnesses remain local; the parent integration records final
run results. No concurrent external-write atomicity, live provider behavior, or filesystem race
resistance is established by these deliberately sequenced fixtures.

Other state targets received focused strategy/oracle inspection, not a complete implementation
audit. The audit checked these mechanisms and retained these limits:

| Target | Existing teeth inspected | Remaining bounded scope |
| --- | --- | --- |
| `invalidation` | Independent reverse-graph queue, sticky staleness, repair/source/frontier checks, reverse-ID DAGs and reopen. | Fixed dependency topology; no concurrent producer admission. |
| `knowledge` | Input candidate/source tables, reverse closure and packet materials. | Fixed topology, current-source catalogs and three role variants. |
| `settlement` | Qualification/cancellation interaction family, evidence-axis truth table and terminal conservation. | Pure reducer evidence, no actual gate/provider execution. |
| `agent_driver` | Explicit kill boundaries, durable event replay, external effect counters and final reopen. | Three tools and scripted provider; domain driver is separate from interactive ProductRunner. |
| `approval` | Signed defect dimensions, warm approve→wrong consume, state/registry binding and durable receipts. | Small credential sets and explicit single fault dimensions. |
| `leases` | Retained stale claims, clock/expiry/overflow schedules, exact claim and CAS checks. | Aggregate transitions; broad concurrent ownership schedules remain separate. |
| `artifacts` | Independent GC/quota arithmetic, saved stale plans, reference reintroduction and real recovery images. | Six content keys; controlled sequential filesystem mutations. |
| `artifact_stream` | Original/object/observed byte model; consumed/unread corruption, limits, terminal failures and drain. | Small files, deliberate in-place writes rather than arbitrary OS schedules. |
| `journal` | Independent event list, aggregate/history readback, observations, rollback and outbox lease/fence state. | Four aggregate identities and bounded synthetic payloads. |
| `projection` | List-derived catalog fold, generation CAS, corruption, failed swap, schema/reopen checks. | One catalog projection family. |
| `migrations` | Checked capacity arithmetic, explicit interruption cuts, restore/retry and idempotent reopen. | Small registry and selected commit boundaries. |
| `scheduler` | Exhaustive feasible pair choice; constructive retry, aging and worker loss histories; durable state comparison. | Small worker/resource domains; no actual distributed worker runtime. |
| `collaboration` | Causal identity permutations and constructive descendant cancellation; independent state and SQLite replay. | Small task/recipient graph and fixed authority fixtures. |
| `harness` | Actual materialization, independent bytes/modes, wrong authority and ancestry checks. | Four paths and at most three materialization rounds. |
| `evidence` | Native admission, causal bundle closure, exact retry, freshness and cancellation/short-write resumes. | Four record slots and selected revision axes. |
