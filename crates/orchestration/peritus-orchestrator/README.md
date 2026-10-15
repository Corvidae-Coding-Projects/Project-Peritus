# peritus-orchestrator

## Folder guide

Durable deterministic writer, gate, reviewer, fixer, and acceptance orchestration.

Active root-workspace package; declarations and targets are in [Cargo.toml](Cargo.toml).

Use [AGENTS.md](AGENTS.md) for file-level responsibilities, child modules, parent context, and nearby implementation owners.

### Local files

| File | Role |
| --- | --- |
| [Cargo.toml](Cargo.toml) | Cargo package/dependency/target declarations for `peritus-orchestrator`. |
| [README.md](README.md) | Human guide and operating/background notes for this folder; AGENTS.md provides the local navigation map. |

### Subfolders

| Folder | Role / next waypoint |
| --- | --- |
| [src/](src/AGENTS.md) | Durable deterministic E0 delivery orchestration for Peritus. |

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

## Behavioral work

Use the [project real-testing policy](../../../README.md#real-testing) and the actual interfaces owned by this crate. The [local map](AGENTS.md) identifies entrypoints and implementation owners.
