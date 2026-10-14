# crates/orchestration/peritus-agent/src — navigation

## Responsibility

Two connected surfaces: DeveloperLoop drives product provider/tool turns; durable AgentDriver/state-machine modules own journal-backed domain-agent execution.

## Files in this folder

| File | Role |
| --- | --- |
| [canonical.rs](canonical.rs) | Domain-canonical encoding used only for deterministic state fences. |
| [command.rs](command.rs) | Closed command vocabulary for one pure agent-turn transition. |
| [completion.rs](completion.rs) | Evidence-bound completion proposals with no effect authority. |
| [control.rs](control.rs) | Exact control-field transitions used by reduction and replay. |
| [developer_interaction.rs](developer_interaction.rs) | Effect-free values shared by the ordinary and verification-only developer compositions. |
| [error.rs](error.rs) | Stable failures returned by pure validation, reduction, and replay. |
| [event.rs](event.rs) | Durable-event representation emitted by the pure reducer. |
| [identity.rs](identity.rs) | Checked identities and immutable turn binding. |
| [lib.rs](lib.rs) | Pure deterministic D0 agent-turn state machine. |
| [limits.rs](limits.rs) | Checked local limits and monotonic counters. |
| [phase.rs](phase.rs) | Closed agent-turn lifecycle phases. |
| [protocol_bridge.rs](protocol_bridge.rs) | Canonical projection into the inert D0 B3 state DTO. |
| [reducer.rs](reducer.rs) | Pure command reduction and exact event replay. |
| [state.rs](state.rs) | Replayable agent-turn aggregate state. |
| [tools.rs](tools.rs) | Ordered, replay-aware projections of C4 tool work. |
| [usage.rs](usage.rs) | Shared numeric usage state and exact overflow-rejecting accumulation. |
| [verified.rs](verified.rs) | Executable invariant/refinement predicates for the local domain. Entry names: `phase_transition_valid`, `PhaseTransitionRejection`, `command_phase_transition`, `required_phase`, `nonterminal_failure`. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [canonical/](canonical/AGENTS.md) | Implementation pieces for `canonical.rs`: Domain-canonical encoding used only for deterministic state fences. |
| [command/](command/AGENTS.md) | Implementation pieces for `command.rs`: Closed command vocabulary for one pure agent-turn transition. |
| [completion/](completion/AGENTS.md) | Implementation pieces for `completion.rs`: Evidence-bound completion proposals with no effect authority. |
| [developer/](developer/AGENTS.md) | Product provider/tool loop: request construction, current host invocation policy, context budgeting, retries, interaction admission, observations, and completion. |
| [runtime/](runtime/AGENTS.md) | Ordinary effect composition around the verified agent reducer. |
| [tools/](tools/AGENTS.md) | Implementation pieces for `tools.rs`: Ordered, replay-aware projections of C4 tool work. |

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
