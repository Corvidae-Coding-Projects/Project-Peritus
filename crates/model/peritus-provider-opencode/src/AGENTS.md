# crates/model/peritus-provider-opencode/src — navigation

## Responsibility

Official OpenCode CLI transport for Zen/Go, executable/model discovery, isolated config/auth, request encoding, JSONL decoding, and normalized failures.

## Files in this folder

| File | Role |
| --- | --- |
| [catalog.rs](catalog.rs) | Model metadata delegated to the vendor's own SDK catalog. |
| [dsml.rs](dsml.rs) | Strict, bounded conversion of terminal DeepSeek DSML tool markup into inert validated calls. |
| [executable.rs](executable.rs) | Explicit discovery and pinning of the official OpenCode executable. |
| [isolation.rs](isolation.rs) | Disposable vendor configuration and short-lived private credential custody, preferring a safe session runtime directory over shared temporary storage. |
| [lib.rs](lib.rs) | Unmodified OpenCode CLI as a tool-disabled Zen/Go inference transport. Entry names: `service_id`, `malformed`. |
| [output.rs](output.rs) | Fail-closed JSONL/tool-envelope normalization and early required-field/basic-type feedback against the declared tool schema. |
| [provider.rs](provider.rs) | Owned vendor inference and inert argument correction; repeated valid-envelope mistakes return to normal host rejection without granting native agent authority. |
| [request.rs](request.rs) | Deterministic transcript, schema-correct argument guidance, inert results, and selectable once-only full-schema delivery for OpenCode. |
| [recovery.rs](recovery.rs) | Fixed malformed-turn reasons, one inert correction prompt, private audit events, and declared usage accounting across correction. |
| [stream.rs](stream.rs) | Buffered normalized events from one final OpenCode structured result. |

## Working in this area

Start at [`lib.rs`](lib.rs) for the module surface, then follow its declarations into the focused files and child waypoints above. Trace at least one caller and the relevant error or recovery path before moving responsibilities; the parent package map identifies the public boundary and adjacent owners.

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
