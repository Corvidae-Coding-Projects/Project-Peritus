# crates/app/peritus-product-runner/src — navigation

## Responsibility

The product execution coordinator: design, writer/reviewer/fixer turns, authorized tools, candidate/gate handling, local context, and final settlement.

## Files in this folder

| File | Role |
| --- | --- |
| [accounting.rs](accounting.rs) | Executable invariant/refinement helpers; older proof correspondence is historical. Entry names: `ProductRunProgress`, `model_requests`, `tool_calls`, `retries`, `provider_failovers`. |
| [api.rs](api.rs) | Explicit public facade; implementation modules retain their existing ownership. |
| [attachment.rs](attachment.rs) | Validation of explicitly selected immutable image bytes, without filesystem authority. |
| [attachment_read.rs](attachment_read.rs) | Exact immutable attachment range request and bounded response. |
| [budget.rs](budget.rs) | Cumulative accounting and caller-selected elapsed horizon for one complete product run. |
| [bundle.rs](bundle.rs) | Bounded deterministic repository context construction. |
| [candidate.rs](candidate.rs) | Exact current candidate paths relative to the managed worktree HEAD. |
| [context_config.rs](context_config.rs) | Explicit local-only context policy and optional bounded subprocess configuration. |
| [context_projection.rs](context_projection.rs) | Public deterministic evidence projection and exact retrieval used by production role turns. |
| [control.rs](control.rs) | Pure durable workbench records and admission decisions, independent of A3 and presentation. |
| [conversation_mode.rs](conversation_mode.rs) | Public conversation policy shared by ordinary and verification-only API surfaces. |
| [delivery_requirement.rs](delivery_requirement.rs) | Deterministic distinction between supporting files and requested live effects. |
| [design.rs](design.rs) | Mandatory repository-grounded design document produced before implementation. |
| [diagnostic.rs](diagnostic.rs) | Best-effort diagnostics for recoverable local command-state failures. |
| [discard_recovery.rs](discard_recovery.rs) | Read-only classification of one explicitly authorized discard transaction. |
| [engineering_workflow.rs](engineering_workflow.rs) | Embedded production-engineering workflow and role skills. |
| [error.rs](error.rs) | Stable redaction-safe product runner failures. |
| [execution.rs](execution.rs) | E0 production writer-gates-review-fixer composition. |
| [failover.rs](failover.rs) | Explicit provider-chain selection after ordinary role recovery is exhausted. |
| [file_metadata.rs](file_metadata.rs) | Portable workspace metadata used by grounding and progress evidence. |
| [gates.rs](gates.rs) | Candidate-aware D1 gate execution adapter. |
| [lib.rs](lib.rs) | Daemon-side product coding-run execution. This crate coordinates concrete provider calls, managed-worktree edits, repository gates, and an independent review/fix cycle. It owns no UI and grants no authority; the daemon supplies… |
| [model_output.rs](model_output.rs) | Shared strict extraction of typed JSON objects from model responses. |
| [progress.rs](progress.rs) | Bounded candidate fingerprints used to replenish productive developer work segments. |
| [qualification.rs](qualification.rs) | Narrow production-tool qualification used by release resilience campaigns. |
| [resource_probe.rs](resource_probe.rs) | Portable run-level memory and workspace-growth observations. |
| [review.rs](review.rs) | Typed independent-review parsing and conserved finding rendering. |
| [reviewer_turn.rs](reviewer_turn.rs) | Fresh tool-capable, read-only independent review turns. |
| [trace.rs](trace.rs) | Durable per-run D0 provider/tool trace. |
| [turn.rs](turn.rs) | Tool-capable writer/fixer turns and independent reviewer prompts. |
| [workspace_delivery.rs](workspace_delivery.rs) | Delivery-specific adapters around the single production execution pipeline. |
| [workspace_filter.rs](workspace_filter.rs) | Stable exclusion of tool-generated dependency and build trees from task candidates. |
| [workspace_kind.rs](workspace_kind.rs) | Caller-resolved workspace delivery boundary, independent of interaction mode. |
| [workspace_media.rs](workspace_media.rs) | Deterministic paged raster-image discovery for grounded model turns. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [accounting/](accounting/AGENTS.md) | Implementation pieces for `accounting.rs`: Executable invariant/refinement helpers; older proof correspondence is historical. Entry names: `ProductRunProgress`, `model_requests`, `tool_calls`, `retries`, `provider_failovers`. |
| [attachment/](attachment/AGENTS.md) | Implementation pieces for `attachment.rs`: Validation of explicitly selected immutable image bytes, without filesystem authority. |
| [budget/](budget/AGENTS.md) | Implementation pieces for `budget.rs`: Cumulative accounting and caller-selected elapsed horizon for one complete product run. |
| [candidate/](candidate/AGENTS.md) | Implementation pieces for `candidate.rs`: Exact current candidate paths relative to the managed worktree HEAD. |
| [context_config/](context_config/AGENTS.md) | Implementation pieces for `context_config.rs`: Explicit local-only context policy and optional bounded subprocess configuration. |
| [control/](control/AGENTS.md) | Implementation pieces for `control.rs`: Pure durable workbench records and admission decisions, independent of A3 and presentation. |
| [design/](design/AGENTS.md) | Implementation pieces for `design.rs`: Mandatory repository-grounded design document produced before implementation. |
| [developer_tools/](developer_tools/AGENTS.md) | Host-owned model tools: schemas, argument admission, workspace grounding, permissions, effect receipts, filesystem/process dispatch, and exact resource readers. |
| [engineering_workflow/](engineering_workflow/AGENTS.md) | Compiled role guidance: production skills, real testing, computer control, role-specific prompt cues, and conditional Rust reliability resources. |
| [execution/](execution/AGENTS.md) | Product-run and conversation entrypoints, live input contracts, phase orchestration, recovery, and settlement. |
| [gates/](gates/AGENTS.md) | Implementation pieces for `gates.rs`: Candidate-aware D1 gate execution adapter. |
| [local_context/](local_context/AGENTS.md) | Durable working-context host: journal/artifact storage, current-policy assembly, exact evidence retrieval, derived entries, compaction, and executor wrapper. |
| [trace/](trace/AGENTS.md) | Implementation pieces for `trace.rs`: Durable per-run D0 provider/tool trace. |
| [turn/](turn/AGENTS.md) | Implementation pieces for `turn.rs`: Tool-capable writer/fixer turns and independent reviewer prompts. |
| [workspace_delivery/](workspace_delivery/AGENTS.md) | Implementation pieces for `workspace_delivery.rs`: Delivery-specific adapters around the single production execution pipeline. |
| [workspace_media/](workspace_media/AGENTS.md) | Implementation pieces for `workspace_media.rs`: Deterministic paged raster-image discovery for grounded model turns. |

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
