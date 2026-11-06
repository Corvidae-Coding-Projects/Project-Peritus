# crates/app/peritus-product-runner/src/design — navigation

## Responsibility

Implementation pieces for `design.rs`: exact source capture, one startup repository-grounding check,
focused domain research with immutable evidence, expansion state, task-plan publication, and
host-validated accepted-design reuse. Grounding completion closes repeat root-wide discovery but not
exact/scoped repository observations or the bounded attachment, web, computer-observation, and
bundled-skill reads required by unresolved design obligations.

## Files in this folder

| File | Role |
| --- | --- |
| [reuse.rs](reuse.rs) | Fresh schema-3 expanded-design reuse with exact source, manifest, output, task, evidence, revision, workspace/request, and policy validation. Schema-1/2 receipts remain readable history but cannot claim expansion. |
| [artifact.rs](artifact.rs) | Deterministic detailed designs for explicit generated-artifact workspaces. |
| [source.rs](source.rs) | Exact source-design acquisition, immutable private publication, identity validation, and bounded verified paging. |
| [expert_pass.rs](expert_pass.rs) | Durable source-bound expert questions and amendments, compact projection, exact draft rendering, host-assigned provenance records, and retained predecessor history. |
| [expert_tools.rs](expert_tools.rs) | Transactional question/amendment, recovery, bounded candidate patch, exact source selection, and publish-by-version dispatch through existing evidence and Ready validation. Tool schemas live in [expert_tools/schema.rs](expert_tools/schema.rs). |
| [expansion.rs](expansion.rs) | Host-owned domain-expansion lifecycle, typed evidence/obligation/acceptance/change/question records, exact answer binding, CAS manifest persistence, deterministic derived Markdown, and artifact-bound ready/partial publication. |
| [research.rs](research.rs) | Design-expansion state tool, bounded model-facing source reads, steering refresh, material-question wait/resume/restart, historical views, validated publication, and required progress checkpoints. |
| [retention.rs](retention.rs) | Private historical source/ledger copies and unvalidated publication proposals retained across steering, rejection, and restart. |
| [research_tests.rs](research_tests.rs) | Filesystem-backed steering, exact source selection, amendment/CAS/restart, evidence repair, large-draft deltas, permissions, context identities, and informed checkpoint scheduling regressions. |
| [qualification.rs](qualification.rs) | Frozen Stage 8 fixture/rubric identity and schema checks used by the deterministic qualification suite. |
| [rollout.rs](rollout.rs) | Internal default-on rollout boundary; recovery-only mode blocks new expansion publication without deleting or reinterpreting retained artifacts or active-run designs. |

Owning module: [design.rs](../design.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
