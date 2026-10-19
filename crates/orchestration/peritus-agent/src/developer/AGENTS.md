# crates/orchestration/peritus-agent/src/developer — navigation

## Responsibility

Product provider/tool loop: request construction, current host invocation policy, context compaction, retries, interaction admission, observations, and completion. Executors can close tools at a phase handoff; terminal requests keep completed tool history valid without advertising new calls.

## Files in this folder

| File | Role |
| --- | --- |
| [accounting.rs](accounting.rs) | Incremental accounting boundaries that survive failed or cancelled invocations. |
| [context.rs](context.rs) | Layered context accounting and digest-bound transcript compaction for developer roles. |
| [context_encoding.rs](context_encoding.rs) | Deterministic token estimates and canonical transcript-source encoding. |
| [context_port.rs](context_port.rs) | Host-owned local context boundary for the production developer loop. |
| [entry.rs](entry.rs) | Public developer-loop entry points and context policy selection. |
| [error.rs](error.rs) | Redaction-safe developer-loop failures. |
| [execution.rs](execution.rs) | Tight provider/tool execution loop. |
| [interaction.rs](interaction.rs) | Optional live user-input and public activity boundary for developer execution. |
| [mod.rs](mod.rs) | Production developer-loop composition around D0's provider runtime. |
| [model_request.rs](model_request.rs) | Provider request construction for developer and semantic-checkpoint turns. |
| [observation.rs](observation.rs) | Preserve tool content while projecting duplicate command receipts and obsolete progress events. |
| [retry.rs](retry.rs) | Attempt-bounded, cancellation-aware provider retries; inference duration does not consume recovery eligibility. |
| [semantic.rs](semantic.rs) | Model-authored semantic checkpoints for long developer transcripts. |
| [tool_executor.rs](tool_executor.rs) | Host-owned tool execution, prerequisite, and liveness boundaries. |
| [types.rs](types.rs) | Checked inputs, outputs, and effect ports for the production developer loop. |
| [usage.rs](usage.rs) | Ordinary provider-counter adaptation for the shared verified usage state. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [execution/](execution/AGENTS.md) | Provider-turn execution and current invocation/retry policy. Preserve current policy separately from compactable history. |

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
