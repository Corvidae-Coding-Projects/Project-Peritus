# crates/app/peritus-product-runner/src/developer_tools — navigation

## Responsibility

Host-owned model tools: schemas, argument admission, workspace grounding, permissions, effect receipts, filesystem/process dispatch, and exact resource readers.

## Files in this folder

| File | Role |
| --- | --- |
| [web.rs](web.rs) | Provider-neutral web schemas, permission-aware dispatch and isolated rendering. |
| [access_policy.rs](access_policy.rs) | Request-derived restrictions for inputs exposed only through a named public interface. |
| [arguments.rs](arguments.rs) | Admission for the closed JSON Schema subset used by the host's workspace tool catalog. This is not a general JSON Schema implementation. Unsupported catalog keywords fail closed; permissions, grounding, filesystem checks and… |
| [attachment_read.rs](attachment_read.rs) | Read-only pages from an exact, daemon-authenticated immutable attachment selection. |
| [bundled_skill.rs](bundled_skill.rs) | Exact read-only pages from bundled skill resources, with no filesystem lookup or grounding. |
| [catalog.rs](catalog.rs) | Provider-neutral workspace tool schemas. |
| [command_budget.rs](command_budget.rs) | Live command allowances derived from the enclosing product-run horizon. |
| [computer.rs](computer.rs) | Provider-neutral browser, native app, and actual image tool integration. |
| [effect.rs](effect.rs) | Shared developer-tool effect mechanics. |
| [evidence.rs](evidence.rs) | Bounded developer-command observations carried into independent review. |
| [executor.rs](executor.rs) | Concrete bounded filesystem and structured-command developer tools. |
| [folder_patch_request.rs](folder_patch_request.rs) | Effect-free target identity for a separately authenticated registered-folder patch. |
| [grounding.rs](grounding.rs) | Deterministic evidence that a model inspected the managed repository. |
| [inspection.rs](inspection.rs) | Bounded read-only workspace inspection operations. |
| [inspection_cancellation.rs](inspection_cancellation.rs) | Run-owned cancellation checks used while streaming filesystem inspection. |
| [inspection_cursor.rs](inspection_cursor.rs) | Scope-bound continuation cursors for bounded filesystem listings and searches. |
| [inspection_list.rs](inspection_list.rs) | Bounded workspace directory listing with scope-bound continuations. |
| [inspection_read.rs](inspection_read.rs) | Byte-bounded, resumable line reads shared by workspace and reference tools. |
| [inspection_search.rs](inspection_search.rs) | Streaming literal workspace search with scoped page continuations. |
| [mod.rs](mod.rs) | Managed-workspace tools exposed to the D0 developer loop. |
| [ownership.rs](ownership.rs) | Workspace file ownership retained across one complete product run. |
| [path.rs](path.rs) | Workspace-relative path validation without ambient traversal. |
| [preview.rs](preview.rs) | Effect-free preview launch and observation values shared by both build modes. |
| [receipt.rs](receipt.rs) | Durable receipts around mutating developer-tool effects. |
| [reference.rs](reference.rs) | Read-only inspection of absolute paths explicitly named by the user's task. |
| [reference_list.rs](reference_list.rs) | Bounded listing of paths explicitly authorized outside the workspace. |
| [reference_path.rs](reference_path.rs) | Native-path resolution for task-named external references. |
| [removal.rs](removal.rs) | Exact, non-recursive workspace removal behavior. |
| [resources.rs](resources.rs) | Resource-aware defaults and admission for developer commands. |
| [wire.rs](wire.rs) | Canonical JSON observations and bounded argument decoding for developer tools. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [command_runtime/](command_runtime/AGENTS.md) | Shared C4/C2 command ownership for product developer tools. |
| [executor/](executor/AGENTS.md) | Concrete workspace executor lifecycle, construction, command ownership, mutations, checkpoints, and progress feedback. |
| [inspection_search/](inspection_search/AGENTS.md) | Implementation pieces for `inspection_search.rs`: Streaming literal workspace search with scoped page continuations. |
| [receipt/](receipt/AGENTS.md) | Implementation pieces for `receipt.rs`: Durable receipts around mutating developer-tool effects. |

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
