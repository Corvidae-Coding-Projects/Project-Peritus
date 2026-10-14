# docs/formal-coverage-evidence/proof-impact-reconciliation — navigation

## Responsibility

Historical evidence for proof impact reconciliation; recorded inputs, outputs and source identities belong to the earlier investigation.

## Files in this folder

| File | Role |
| --- | --- |
| [SHA256SUMS](SHA256SUMS) | Recorded SHA-256 inventory for neighboring retained input/artifact files; keep bytes and digest entries consistent. |
| [authorization-audit.md](authorization-audit.md) | Document: GAP-02 proof-impact authorization audit. |
| [candidate-inputs.json](candidate-inputs.json) | Structured candidate inputs data with fields `hash_algorithm`, `limits`, `schema`, `schema_version`, `sources`. |
| [clippy.log](clippy.log) | Retained evidence/output for clippy. |
| [docs-check.log](docs-check.log) | Retained evidence/output for docs check. |
| [evidence-audit.md](evidence-audit.md) | Document: GAP-02 proof-impact, review, and obligation evidence audit. |
| [protected-inputs.json](protected-inputs.json) | Structured protected inputs data with fields `hash_algorithm`, `limits`, `schema`, `schema_version`, `sources`. |
| [protected-trust.log](protected-trust.log) | Retained evidence/output for protected trust. |
| [reconcile-tests.log](reconcile-tests.log) | Retained evidence/output for reconcile tests. |
| [reconcile.log](reconcile.log) | Retained evidence/output for reconcile. |
| [reconcile.py](reconcile.py) | Produce an audit-only reconciliation; never write an approval or verification register. |
| [reconcile_tests.py](reconcile_tests.py) | Disposable Git probes for the audit script's input and publication boundaries. |
| [reconciler-review.md](reconciler-review.md) | Document: GAP-02 audit exporter and reconciler review. |
| [reconciliation.json](reconciliation.json) | Structured reconciliation data with fields `affected_packages`, `approved_change_ids`, `approved_history_preserved`, `authority`, `branch_source_transitions`. |
| [review-provenance.json](review-provenance.json) | Structured review provenance data with fields `schema`, `schema_version`, `status`, `actors_registered`, `final_candidate_approval`. |
| [run_gates.py](run_gates.py) | Run and retain the 132 exact package gates on one clean, frozen candidate. |
| [validation.json](validation.json) | Structured validation data with fields `schema`, `schema_version`, `authority`, `source_candidate`, `protected_base`. |
| [xtask-tests.log](xtask-tests.log) | Retained evidence/output for xtask tests. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [prior-audit-source/](prior-audit-source/AGENTS.md) | Historical evidence for prior audit source; recorded inputs, outputs and source identities belong to the earlier investigation. |

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
