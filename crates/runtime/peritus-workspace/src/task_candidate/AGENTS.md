# Task candidate workspace runtime

## Responsibility

[managed.rs](managed.rs) and [managed/](managed/AGENTS.md) own exact task preimages, Git/index/content fingerprints, nested repository patch export, and crash-safe discard transactions. [discard_recovery.rs](discard_recovery.rs) defines read-only transaction state. The parent `task_candidate.rs` exposes the stable candidate identity and error API shared by clients and the product runner.

These functions operate only on caller-authorized paths; they do not admit work, run models, or grant execution permissions. The original workspace creation API remains in `../candidate.rs`.
## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.
