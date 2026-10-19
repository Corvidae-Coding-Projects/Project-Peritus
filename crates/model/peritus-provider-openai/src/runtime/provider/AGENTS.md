# crates/model/peritus-provider-openai/src/runtime/provider — navigation

## Responsibility

Implementation pieces for `provider.rs`: One-turn ownership of the pre-authenticated Codex executable.

## Files in this folder

| File | Role |
| --- | --- |
| [cached.rs](cached.rs) | Finite official app-server routing, prediction forks, direct content-free progress, usage/errors and safe pre-inference fallback; notices stay outside the final decoder transcript. |
| [failure.rs](failure.rs) | Redaction-safe runtime terminal classification. |
| [invocation.rs](invocation.rs) | Hardened authentication and isolated one-turn process projections. |

Owning module: [provider.rs](../provider.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../AGENTS.md) · [Repository map](../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
