# crates/runtime/peritus-sandbox/src — navigation

## Responsibility

Checked platform-neutral sandbox contracts and executable reference semantics.

## Files in this folder

| File | Role |
| --- | --- |
| [admission.rs](admission.rs) | Fail-closed backend admission. |
| [backend.rs](backend.rs) | Platform-neutral backend descriptors and preparation trait. |
| [binding.rs](binding.rs) | Exact target identity bound into every sandbox plan. |
| [cancellation.rs](cancellation.rs) | Idempotent, first-reason-wins cancellation. |
| [canonical.rs](canonical.rs) | Versioned canonical bytes for sandbox and backend identities. |
| [contract.rs](contract.rs) | Aggregate sandbox capability contract. |
| [environment.rs](environment.rs) | Explicit execution-environment contracts. |
| [error.rs](error.rs) | Stable sandbox failures and recovery guidance. |
| [feature.rs](feature.rs) | Closed backend enforcement-feature vocabulary. |
| [filesystem.rs](filesystem.rs) | Logical filesystem capability contracts. |
| [lib.rs](lib.rs) | Checked platform-neutral sandbox contracts and executable reference semantics. |
| [lifecycle.rs](lifecycle.rs) | Closed sandbox-session lifecycle. |
| [network.rs](network.rs) | Deny-by-default network capability contracts. |
| [observation.rs](observation.rs) | Bounded backend enforcement observations. |
| [plan.rs](plan.rs) | Checked sandbox plan compilation. |
| [process_policy.rs](process_policy.rs) | Process-tree capability contracts. |
| [refinement.rs](refinement.rs) | Named C2 sandbox refinement obligations. |
| [requirements.rs](requirements.rs) | Concrete effects required by one planned invocation. |
| [resource.rs](resource.rs) | Checked resource budgets and reference-backend accounting. |
| [secret.rs](secret.rs) | Secret references and delivery contracts. Secret values never enter this crate. |
| [terminal.rs](terminal.rs) | Pipe and pseudo-terminal contracts. |
| [verified.rs](verified.rs) | Executable invariant/refinement predicates for the local domain. Entry names: `CompilationFacts`, `compilation_complete`, `BackendFacts`, `backend_complete`, `resource_charge_allowed`. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [reference/](reference/AGENTS.md) | Deterministic executable backend for unit and cross-platform conformance tests. |

## Working in this area

Start at [`lib.rs`](lib.rs) for the module surface, then follow its declarations into the focused files and child waypoints above. Trace at least one caller and the relevant error or recovery path before moving responsibilities; the parent package map identifies the public boundary and adjacent owners.

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
