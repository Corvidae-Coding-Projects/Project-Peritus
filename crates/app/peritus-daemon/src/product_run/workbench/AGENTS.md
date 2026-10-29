# crates/app/peritus-daemon/src/product_run/workbench — navigation

## Responsibility

Daemon ownership of workbench conversations, sessions, revisions, review constraints, and UI-facing operations.

## Files in this folder

| File | Role |
| --- | --- |
| [brief.rs](brief.rs) | Authenticated, read-only task-brief projection from the authoritative input ledger. |
| [checkpoints.rs](checkpoints.rs) | Bounded workspace checkpoints and exact-preimage rewind transactions. |
| [conversation.rs](conversation.rs) | Read-only authenticated discovery of the durable conversation execution. |
| [execution.rs](execution.rs) | Explicit fresh execution admission. Legacy state is never implicitly upgraded or resumed. |
| [files.rs](files.rs) | Authorized workspace-file preview, confirmation and retained selection inspection. |
| [folder_mutation.rs](folder_mutation.rs) | Authenticated reviewed folder patches through committed G4/B0/B1 authority and C1 ownership. |
| [fork.rs](fork.rs) | Exact parent/checkpoint lineage validation and atomic non-running child creation. |
| [goal.rs](goal.rs) | Persistent-goal A3 projection, resume coordination, and safe cancellation signalling. |
| [guidance.rs](guidance.rs) | Exact A3 guidance conversion into independent G4 audit and C0 sidecar mutations. |
| [images.rs](images.rs) | Authenticated image preview and confirmed atomic import; no ambient files or provider sends. |
| [inputs.rs](inputs.rs) | Authenticated queue mapping; host-only incorporation has no A3 intent variant. |
| [launch.rs](launch.rs) | Durable admission and daemon-owned preview launch, interaction, capture, and inspection. |
| [mapping.rs](mapping.rs) | Exact authenticated public-intent to durable domain-operation mapping. |
| [review.rs](review.rs) | Structured review projection and exact-anchor mutation admission. |
| [run_control.rs](run_control.rs) | Authenticated adaptation of run dashboard controls to durable conversation authority. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [brief/](brief/AGENTS.md) | Implementation pieces for `brief.rs`: Authenticated, read-only task-brief projection from the authoritative input ledger. |
| [checkpoints/](checkpoints/AGENTS.md) | Implementation pieces for `checkpoints.rs`: Bounded workspace checkpoints and exact-preimage rewind transactions. |
| [files/](files/AGENTS.md) | Implementation pieces for `files.rs`: Authorized workspace-file preview, confirmation and retained selection inspection. |
| [folder_mutation/](folder_mutation/AGENTS.md) | Implementation pieces for `folder_mutation.rs`: Authenticated reviewed folder patches through committed G4/B0/B1 authority and C1 ownership. |
| [images/](images/AGENTS.md) | Implementation pieces for `images.rs`: Authenticated image preview and confirmed atomic import; no ambient files or provider sends. |
| [launch/](launch/AGENTS.md) | Implementation pieces for `launch.rs`: Durable admission and daemon-owned preview launch, interaction, capture, and inspection. |
| [mapping/](mapping/AGENTS.md) | Implementation pieces for `mapping.rs`: Exact authenticated public-intent to durable domain-operation mapping. |
| [review/](review/AGENTS.md) | Implementation pieces for `review.rs`: Structured review projection and exact-anchor mutation admission. |

Owning module: [workbench.rs](../workbench.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../AGENTS.md) · [Repository map](../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
