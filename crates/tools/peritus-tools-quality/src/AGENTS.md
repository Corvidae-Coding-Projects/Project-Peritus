# crates/tools/peritus-tools-quality/src — navigation

## Responsibility

Explicit quality-check discovery, invocation, and candidate B2 evidence.

## Files in this folder

| File | Role |
| --- | --- |
| [acceptance.rs](acceptance.rs) | Exact acceptance-plan bindings derived from quality definitions. |
| [catalog.rs](catalog.rs) | Exact deterministic quality tool descriptors. |
| [definition.rs](definition.rs) | Explicit, bounded quality check definitions. |
| [discovery.rs](discovery.rs) | Deterministic explicit/Cargo/Just quality catalog discovery. |
| [dispatcher.rs](dispatcher.rs) | Router-permit-only quality dispatchers. |
| [error.rs](error.rs) | Stable quality-tool errors. |
| [execution.rs](execution.rs) | Owned quality process projection into the C4 active-execution protocol. |
| [input.rs](input.rs) | Checked quality-run selection input. |
| [json_value.rs](json_value.rs) | Explicit JSON value assembly for ordinary quality projections. |
| [lib.rs](lib.rs) | Explicit quality-check discovery, invocation, and candidate B2 evidence. |
| [observation.rs](observation.rs) | Structured execution facts and candidate B2 gate observation inputs. |
| [parser.rs](parser.rs) | Complete bounded quality output parsing. |
| [plan.rs](plan.rs) | Complete restricted C2 plan compilation for one selected check. |
| [render.rs](render.rs) | Bounded control-safe quality result rendering. |
| [result.rs](result.rs) | Strict typed decoding of `quality.run` terminal envelopes. |
| [snapshot.rs](snapshot.rs) | Clean immutable C1 snapshot binding for quality execution. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [discovery/](discovery/AGENTS.md) | Implementation pieces for `discovery.rs`: Deterministic explicit/Cargo/Just quality catalog discovery. |
| [dispatcher/](dispatcher/AGENTS.md) | Implementation pieces for `dispatcher.rs`: Router-permit-only quality dispatchers. |
| [execution/](execution/AGENTS.md) | Implementation pieces for `execution.rs`: Owned quality process projection into the C4 active-execution protocol. |
| [result/](result/AGENTS.md) | Implementation pieces for `result.rs`: Strict typed decoding of `quality.run` terminal envelopes. |

## Working in this area

Start at [`lib.rs`](lib.rs) for the module surface, then follow its declarations into the focused files and child waypoints above. Trace at least one caller and the relevant error or recovery path before moving responsibilities; the parent package map identifies the public boundary and adjacent owners.

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
