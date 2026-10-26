# crates/model/peritus-provider-openai/src/runtime — navigation

## Responsibility

Implementation pieces for `runtime.rs`: Constrained account-backed transport through `OpenAI`'s credential-owning `Codex` executable.

## Files in this folder

| File | Role |
| --- | --- |
| [config.rs](config.rs) | Immutable Codex runtime configuration and narrow capability validation. |
| [executable.rs](executable.rs) | Discovery and immutable pinning of `OpenAI`'s unmodified `codex` executable. |
| [live.rs](live.rs) | Pull-driven public summaries and throttled content-free runtime progress; structured tool proposals wait for final validation. |
| [output.rs](output.rs) | Fail-closed decoding of the bounded Codex JSONL turn transcript. |
| [provider.rs](provider.rs) | One-turn ownership of the pre-authenticated Codex executable. |
| [request.rs](request.rs) | Deterministic transcript projection for the credential-owning Codex router. |
| [stream.rs](stream.rs) | Normalized final events from one validated Codex JSONL terminal, following any live public summaries. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [output/](output/AGENTS.md) | Implementation pieces for `output.rs`: Fail-closed decoding of the bounded Codex JSONL turn transcript. |
| [provider/](provider/AGENTS.md) | Implementation pieces for `provider.rs`: One-turn ownership of the pre-authenticated Codex executable. |
| [request/](request/AGENTS.md) | Implementation pieces for `request.rs`: Deterministic transcript projection for the credential-owning Codex router. |

Owning module: [runtime.rs](../runtime.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
