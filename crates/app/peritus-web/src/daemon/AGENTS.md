# crates/app/peritus-web/src/daemon — navigation

## Responsibility

Implementation pieces for `daemon.rs`: Native conversation adapter over the existing negotiated application client.

## Files in this folder

| File | Role |
| --- | --- |
| [chat.rs](chat.rs) | Native text messages use the same durable conversation ledger as the terminal client. |
| [chat_upload.rs](chat_upload.rs) | Session-scoped transfers of immutable message and attachment snapshots. |
| [conversation.rs](conversation.rs) | Honest observation of a run-backed or prepared durable conversation. |
| [improvements.rs](improvements.rs) | Candidate inbox bridge; mutations retain the original native request and response. |
| [readiness.rs](readiness.rs) | Saved launcher metadata is not proof that the running daemon admits a workspace. |
| [receipts.rs](receipts.rs) | Retain exact native requests before transmission. Never infer acceptance from connectivity. |

Owning module: [daemon.rs](../daemon.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
