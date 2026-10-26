# crates/state/peritus-leases/src — navigation

## Responsibility

Generation-bound exclusive mutation leases for Peritus.

## Files in this folder

| File | Role |
| --- | --- |
| [binding.rs](binding.rs) | Exact unprivileged command projections carried through durable lease plans. |
| [claim.rs](claim.rs) | Unprivileged exact lease claims. |
| [command.rs](command.rs) | Checked logical lease command values. |
| [failure.rs](failure.rs) | Stable fail-closed failures and recovery guidance. |
| [lib.rs](lib.rs) | Generation-bound exclusive mutation leases for Peritus. |
| [model.rs](model.rs) | Abstract reachability model and mathematical lease predicates. |
| [port.rs](port.rs) | Durable compare-and-swap request and untrusted observation contracts. |
| [reconcile.rs](reconcile.rs) | Exact fencing evidence and post-fence reconciliation observations. |
| [scope.rs](scope.rs) | Exact lease scope, holder identity, and checked duration values. |
| [state.rs](state.rs) | Privately represented aggregate state and checked immutable projections. |
| [transition.rs](transition.rs) | Pure checked reducers and move-only logical transition values. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [binding/](binding/AGENTS.md) | Implementation pieces for `binding.rs`: Exact unprivileged command projections carried through durable lease plans. |
| [command/](command/AGENTS.md) | Implementation pieces for `command.rs`: Checked logical lease command values. |
| [model/](model/AGENTS.md) | Implementation pieces for `model.rs`: Abstract reachability model and mathematical lease predicates. |
| [port/](port/AGENTS.md) | Implementation pieces for `port.rs`: Durable compare-and-swap request and untrusted observation contracts. |
| [proofs/](proofs/AGENTS.md) | Proof decomposition for logical exclusivity, fencing, exact authority, and refinement. |
| [state/](state/AGENTS.md) | Implementation pieces for `state.rs`: Privately represented aggregate state and checked immutable projections. |
| [transition/](transition/AGENTS.md) | Implementation pieces for `transition.rs`: Pure checked reducers and move-only logical transition values. |

## Working in this area

Start at [`lib.rs`](lib.rs) for the module surface, then follow its declarations into the focused files and child waypoints above. Trace at least one caller and the relevant error or recovery path before moving responsibilities; the parent package map identifies the public boundary and adjacent owners.

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
