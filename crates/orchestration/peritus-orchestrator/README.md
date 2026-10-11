# peritus-orchestrator

`peritus-orchestrator` owns the durable E0 delivery lifecycle. It composes D0 turns, D1 gates,
D2 review, D3 scheduling and collaboration, B2 acceptance evaluation, and B0 lifecycle truth into
the closed writer -> gates -> reviewer -> fixer loop.

The crate is an event-sourced run aggregate with commit-before-effect directives. It never owns
provider, process, workspace, policy, waiver, or acceptance authority. Exact role, candidate,
revision, evidence, child-head, and command bindings make pause, cancellation, replay, recovery,
and terminal outcomes deterministic.

The production-facing coordinator derives only `Accept`, `Fix`, or `Exhausted` from an exact D1
report and conserved D2 ledger. `Accept` is available only when target coverage and all command
observations pass and no policy blocker remains; fixer completion alone never grants acceptance.

See [the E0 design](../../../.design/e0-actor-orchestrator.md) for the frozen contract and
`docs/e0-actor-orchestrator.md` for operational use.

## Focused checks

From the repository root:

```sh
CARGO_BUILD_JOBS=2 cargo test --locked --package peritus-orchestrator
```
