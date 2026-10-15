# crates/foundation/peritus-release-policy/src/decision — navigation

## Responsibility

Implementation pieces for `decision.rs`: Canonical assessments, diagnostics, and non-authorizing release verdict.

## Files in this folder

| File | Role |
| --- | --- |
| [assessment.rs](assessment.rs) | Canonical artifact, criterion, and qualification assessments. |
| [completeness.rs](completeness.rs) | Executable refinements of final assessment completeness. |
| [diagnostic.rs](diagnostic.rs) | Canonical fail-closed diagnostic vocabulary. |
| [digest.rs](digest.rs) | Deterministic fingerprint reduction over canonical assessments. |
| [finding.rs](finding.rs) | Aggregated finding and waiver assessment. |
| [review.rs](review.rs) | Aggregated independent-review assessment. |

Owning module: [decision.rs](../decision.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
