# crates/model/peritus-provider-core/src — navigation

## Responsibility

Provider-independent owned transport primitives for Peritus.

## Files in this folder

| File | Role |
| --- | --- |
| [adapter.rs](adapter.rs) | Provider-neutral adapter ownership contract, metadata preparation, and effective request profiles. |
| [backoff.rs](backoff.rs) | Cancellation-aware execution of checked backoff delays. |
| [cancellation.rs](cancellation.rs) | Race-free cooperative cancellation without a runtime type in the public API. |
| [catalog.rs](catalog.rs) | Provider-owned model discovery and route-matched numeric capacity resolution; no bundled model names. |
| [connection.rs](connection.rs) | Explicit, bounded connection qualification without application tools or workspace access. |
| [continuation.rs](continuation.rs) | Persisted continuation bindings restored into provider-owned runtime state. |
| [credential.rs](credential.rs) | Narrow, zeroizing credential resolution and HTTP projection. |
| [diagnostic.rs](diagnostic.rs) | Allowlisted, bounded, redaction-aware diagnostics. |
| [effort.rs](effort.rs) | Immutable effort selection without changing credentials, routing, or in-flight requests. |
| [endpoint.rs](endpoint.rs) | Validated HTTP(S) endpoints. |
| [error.rs](error.rs) | Stable redaction-safe provider-core errors. |
| [framing.rs](framing.rs) | Incremental bounded Server-Sent Events and newline-delimited JSON framing. |
| [healing.rs](healing.rs) | Bounded syntax-only repair of complete model-authored JSON, never transport framing. |
| [hosted.rs](hosted.rs) | Reviewed hosted API identities and operation routes; never a fallback model catalog. |
| [http.rs](http.rs) | Bounded owned HTTP request, response, status, and header values. |
| [lib.rs](lib.rs) | Provider-independent owned transport primitives for Peritus. |
| [process.rs](process.rs) | Bounded owned subprocess requests with a runtime-private Tokio implementation. |
| [projection.rs](projection.rs) | Content-free measurements of actual adapter-rendered parts and a hashed prefix lower bound; never provider token or cache claims. |
| [process_containment.rs](process_containment.rs) | Host process containment for credential-owning provider executables. |
| [qualification.rs](qualification.rs) | Provider role requirements and user-authorized route selection. |
| [recovery.rs](recovery.rs) | Stable provider terminal causes and phase-local recovery dispositions. |
| [redaction.rs](redaction.rs) | Bounded values whose formatting is always redacted. |
| [reqwest_transport.rs](reqwest_transport.rs) | Default Reqwest/Rustls transport with ambient proxies, redirects, and implicit retries disabled. |
| [retry.rs](retry.rs) | Deterministic bounded retry legality and delay planning. |
| [stream.rs](stream.rs) | Owned normalized model-event streams. |
| [transport.rs](transport.rs) | Object-safe standard-library-future HTTP transport and body interfaces. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [catalog/](catalog/AGENTS.md) | Implementation pieces for `catalog.rs`: Provider-owned model discovery and route-matched numeric capacity resolution; no bundled model names. |
| [framing/](framing/AGENTS.md) | Implementation pieces for `framing.rs`: Incremental bounded Server-Sent Events and newline-delimited JSON framing. |
| [healing/](healing/AGENTS.md) | Implementation pieces for `healing.rs`: Bounded syntax-only repair of complete model-authored JSON, never transport framing. |
| [hosted/](hosted/AGENTS.md) | Implementation pieces for `hosted.rs`: Reviewed hosted API identities and operation routes; never a fallback model catalog. |
| [http/](http/AGENTS.md) | Implementation pieces for `http.rs`: Bounded owned HTTP request, response, status, and header values. |
| [process/](process/AGENTS.md) | Implementation pieces for `process.rs`: Bounded owned subprocess requests with a runtime-private Tokio implementation. |
| [projection/](projection/AGENTS.md) | Disjoint content-free component measurements over actual adapter-rendered byte spans. |
| [qualification/](qualification/AGENTS.md) | Implementation pieces for `qualification.rs`: Provider role requirements and user-authorized route selection. |

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
