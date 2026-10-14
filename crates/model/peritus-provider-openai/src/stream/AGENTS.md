# crates/model/peritus-provider-openai/src/stream — navigation

## Responsibility

Implementation pieces for `stream.rs`: Bounded `OpenAI` Responses SSE ownership and normalized event emission.

## Files in this folder

| File | Role |
| --- | --- |
| [decode.rs](decode.rs) | Bounded event-envelope parsing, sequence validation, and event dispatch. |
| [metadata.rs](metadata.rs) | Bounded response-header observations and HTTP error classification. |
| [output.rs](output.rs) | Heterogeneous output-item, content-part, reasoning, and tool-fragment normalization. |
| [state.rs](state.rs) | Private item/content assembly and provider sequence state. |
| [terminal.rs](terminal.rs) | Response lifecycle, usage/cache, and provider terminal normalization. |

Owning module: [stream.rs](../stream.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
