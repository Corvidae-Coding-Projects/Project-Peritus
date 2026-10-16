# crates/app/peritus-product-runner/src/turn — navigation

## Responsibility

Implementation pieces for `turn.rs`: Tool-capable writer/fixer turns and independent reviewer prompts.

## Files in this folder

| File | Role |
| --- | --- |
| [correction.rs](correction.rs) | Corrective context for rejected or prematurely stopped developer turns. |
| [evidence.rs](evidence.rs) | Provider-aware developer/reviewer evidence previews; preserve exact source identities and page retrieval when binary content cannot be embedded in model text. |
| [provider.rs](provider.rs) | Product-role recovery after one developer invocation reaches a provider terminal. |
| [request_name.rs](request_name.rs) | Stable provider-request identities for one product role and cycle. |
| [reviewer_prompt.rs](reviewer_prompt.rs) | Independent reviewer instructions and exact evidence projection. |
| [terminal.rs](terminal.rs) | Strict task-level developer terminal decoding. |
| [writer_prompt.rs](writer_prompt.rs) | Writer and fixer prompts for the developer loop. |

Owning module: [turn.rs](../turn.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
