# crates/model/peritus-provider-opencode/src — navigation

## Responsibility

Official OpenCode CLI transport for Zen/Go, executable/model discovery, isolated config/auth, request encoding, JSONL decoding, and normalized failures.

## Files in this folder

| File | Role |
| --- | --- |
| [catalog.rs](catalog.rs) | Model metadata delegated to the vendor's own SDK catalog. |
| [executable.rs](executable.rs) | Explicit discovery and pinning of the official OpenCode executable. |
| [isolation.rs](isolation.rs) | Disposable vendor configuration and short-lived private credential custody. |
| [lib.rs](lib.rs) | Unmodified OpenCode CLI as a tool-disabled Zen/Go inference transport. Entry names: `service_id`, `malformed`. |
| [output.rs](output.rs) | Fail-closed JSONL event and inert tool-envelope normalization. |
| [provider.rs](provider.rs) | One owned vendor process per Peritus model turn, with no native agent authority. |
| [request.rs](request.rs) | Deterministic transcript and inert structured-result projection for OpenCode. |
| [stream.rs](stream.rs) | Buffered normalized events from one final OpenCode structured result. |

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
