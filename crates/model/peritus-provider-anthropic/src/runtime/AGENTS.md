# crates/model/peritus-provider-anthropic/src/runtime — navigation

## Responsibility

Implementation pieces for `runtime.rs`: Constrained account-backed transport through Anthropic's credential-owning executable.

## Files in this folder

| File | Role |
| --- | --- |
| [config.rs](config.rs) | Immutable Claude runtime configuration and narrow capability validation. |
| [executable.rs](executable.rs) | Discovery and pinning of Anthropic's unmodified `claude` executable. |
| [output.rs](output.rs) | Fail-closed decoding of Claude's final structured result envelope. |
| [provider.rs](provider.rs) | Authentication delegation and one-turn Claude executable ownership. |
| [request.rs](request.rs) | Deterministic transcript and inert structured-result projection for Claude. |
| [stream.rs](stream.rs) | Buffered normalized events from one final Claude structured result. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [output/](output/AGENTS.md) | Implementation pieces for `output.rs`: Fail-closed decoding of Claude's final structured result envelope. |
| [provider/](provider/AGENTS.md) | Implementation pieces for `provider.rs`: Authentication delegation and one-turn Claude executable ownership. |

Owning module: [runtime.rs](../runtime.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
