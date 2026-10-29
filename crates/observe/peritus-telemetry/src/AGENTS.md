# crates/observe/peritus-telemetry/src — navigation

## Responsibility

Bounded OpenTelemetry-compatible projections and exporters for Peritus.

## Files in this folder

| File | Role |
| --- | --- |
| [error.rs](error.rs) | Stable content-free telemetry failure vocabulary. |
| [lib.rs](lib.rs) | Bounded OpenTelemetry-compatible projections and exporters for Peritus. |
| [metrics.rs](metrics.rs) | Stable metric names, points, and checked cumulative state. |
| [projection.rs](projection.rs) | OpenTelemetry-compatible and metric projection from checked trace state. |
| [recovery.rs](recovery.rs) | Deterministic restart recovery from a projection and final-disposition checkpoint. |
| [storage.rs](storage.rs) | Durable, atomic export-checkpoint storage. |
| [verified.rs](verified.rs) | Executable invariant/refinement predicates for the local domain. Entry names: `BufferFacts`, `bounded_accounting`, `counters_monotonic`, `acknowledgement_legal`, `recovery_prefix_legal`. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [buffer/](buffer/AGENTS.md) | Deterministic bounded buffer and drop accounting. |
| [export/](export/AGENTS.md) | Stable idempotent exporter batches, acknowledgements, and shutdown pumping. |

## Working in this area

Start at [`lib.rs`](lib.rs) for the module surface, then follow its declarations into the focused files and child waypoints above. Trace at least one caller and the relevant error or recovery path before moving responsibilities; the parent package map identifies the public boundary and adjacent owners.

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
