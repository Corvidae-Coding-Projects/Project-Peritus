# crates/orchestration/peritus-context/src/selection — navigation

## Responsibility

Implementation pieces for `selection.rs`: Deterministic required-first selection with atomic optional closure admission.

## Files in this folder

| File | Role |
| --- | --- |
| [certificate.rs](certificate.rs) | Independent deterministic replay certificate for a complete selection result. |
| [closure.rs](closure.rs) | Visits actual reachable dependencies once and retains canonical closure order for atomic selection and diagnostics. |
| [ordering.rs](ordering.rs) | Deterministic optional-root ranking and final render ordering. |
| [outcome.rs](outcome.rs) | Independent logical success relation and executable required-node check. |
| [plan.rs](plan.rs) | Transactional required-first and optional context planning. |

Owning module: [selection.rs](../selection.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
