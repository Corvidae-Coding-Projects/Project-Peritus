# crates/foundation/peritus-security-policy/src/evaluator — navigation

## Responsibility

Implementation pieces for `evaluator.rs`: Total effect-free security-readiness evaluation.

## Files in this folder

| File | Role |
| --- | --- |
| [artifacts.rs](artifacts.rs) | Required canonical evidence-manifest role checks. |
| [binding.rs](binding.rs) | SEC-INV-001 executable exact-candidate checks. |
| [controls.rs](controls.rs) | R-SEC and numbered acceptance-criterion completeness. |
| [inventories.rs](inventories.rs) | Threat, control, unsafe-code, and trusted-computing-base inventory checks. |
| [review.rs](review.rs) | Independent review completion and release-blocking finding closure. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [artifacts/](artifacts/AGENTS.md) | Implementation pieces for `artifacts.rs`: Required canonical evidence-manifest role checks. |
| [controls/](controls/AGENTS.md) | Implementation pieces for `controls.rs`: R-SEC and numbered acceptance-criterion completeness. |
| [inventories/](inventories/AGENTS.md) | Implementation pieces for `inventories.rs`: Threat, control, unsafe-code, and trusted-computing-base inventory checks. |
| [review/](review/AGENTS.md) | Implementation pieces for `review.rs`: Independent review completion and release-blocking finding closure. |

Owning module: [evaluator.rs](../evaluator.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
